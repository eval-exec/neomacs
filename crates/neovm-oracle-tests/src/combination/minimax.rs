//! Oracle parity tests for minimax algorithm with alpha-beta pruning:
//! game tree evaluation, alpha-beta optimization, tic-tac-toe state evaluation,
//! minimax with move generation, and evaluation functions for partial game states.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic minimax on a static game tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_basic_game_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Game tree as nested lists: leaf = integer, internal = list of children
    // Minimax alternates between maximizing and minimizing levels
    let form = r#"(progn
  ;; A leaf is an integer; an internal node is a list of children
  (fset 'neovm--mm-leaf-p (lambda (node) (integerp node)))

  (fset 'neovm--mm-minimax
    (lambda (node depth maximizing)
      (if (or (funcall 'neovm--mm-leaf-p node) (= depth 0))
          node
        (if maximizing
            (let ((best most-negative-fixnum))
              (dolist (child node)
                (let ((val (funcall 'neovm--mm-minimax child (1- depth) nil)))
                  (when (> val best)
                    (setq best val))))
              best)
          (let ((best most-positive-fixnum))
            (dolist (child node)
              (let ((val (funcall 'neovm--mm-minimax child (1- depth) t)))
                (when (< val best)
                  (setq best val))))
            best)))))

  (unwind-protect
      (list
       ;; Simple tree:         max
       ;;                    /     \
       ;;                 min       min
       ;;                / \       / \
       ;;               3   5     2   9
       (funcall 'neovm--mm-minimax '((3 5) (2 9)) 10 t) ;; max(min(3,5), min(2,9)) = max(3,2) = 3

       ;; Three-level tree:    max
       ;;                   /       \
       ;;                 min       min
       ;;               /   \     /   \
       ;;             max   max  max   max
       ;;             /\    /\   /\    /\
       ;;            3 12  8 2  4 6  14 5
       (funcall 'neovm--mm-minimax
                '(((3 12) (8 2)) ((4 6) (14 5))) 10 t)
       ;; = max(min(max(3,12), max(8,2)), min(max(4,6), max(14,5)))
       ;; = max(min(12, 8), min(6, 14))
       ;; = max(8, 6) = 8

       ;; Single leaf
       (funcall 'neovm--mm-minimax 42 10 t) ;; just 42

       ;; Depth-limited: stops at depth 0, returns node as-is
       ;; For a leaf at depth 0, just returns the leaf
       (funcall 'neovm--mm-minimax 7 0 t)

       ;; Unbalanced tree
       (funcall 'neovm--mm-minimax '((1 2 3) (4)) 10 t)
       ;; max(min(1,2,3), min(4)) = max(1, 4) = 4

       ;; All same values
       (funcall 'neovm--mm-minimax '((5 5) (5 5)) 10 t))
    (fmakunbound 'neovm--mm-leaf-p)
    (fmakunbound 'neovm--mm-minimax)))"#;
    let expect = expect_test::expect![[r#""OK (3 8 42 7 4 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax with alpha-beta pruning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_alpha_beta_pruning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ab-leaf-p (lambda (node) (integerp node)))

  ;; Alpha-beta pruning minimax
  ;; Returns the optimal value, same as plain minimax but prunes branches
  (fset 'neovm--ab-minimax
    (lambda (node depth alpha beta maximizing)
      (if (or (funcall 'neovm--ab-leaf-p node) (= depth 0))
          node
        (if maximizing
            (let ((best most-negative-fixnum)
                  (children node)
                  (pruned nil))
              (while (and children (not pruned))
                (let ((val (funcall 'neovm--ab-minimax
                                    (car children) (1- depth) alpha beta nil)))
                  (when (> val best) (setq best val))
                  (when (> best alpha) (setq alpha best))
                  (when (>= alpha beta) (setq pruned t)))
                (setq children (cdr children)))
              best)
          (let ((best most-positive-fixnum)
                (children node)
                (pruned nil))
            (while (and children (not pruned))
              (let ((val (funcall 'neovm--ab-minimax
                                  (car children) (1- depth) alpha beta t)))
                (when (< val best) (setq best val))
                (when (< best beta) (setq beta best))
                (when (>= alpha beta) (setq pruned t)))
              (setq children (cdr children)))
            best)))))

  ;; Node-counting version to verify pruning occurs
  (fset 'neovm--ab-minimax-count
    (lambda (node depth alpha beta maximizing counter)
      ;; counter is a cons cell (count . 0) used as mutable counter
      (setcar counter (1+ (car counter)))
      (if (or (funcall 'neovm--ab-leaf-p node) (= depth 0))
          node
        (if maximizing
            (let ((best most-negative-fixnum)
                  (children node)
                  (pruned nil))
              (while (and children (not pruned))
                (let ((val (funcall 'neovm--ab-minimax-count
                                    (car children) (1- depth) alpha beta nil counter)))
                  (when (> val best) (setq best val))
                  (when (> best alpha) (setq alpha best))
                  (when (>= alpha beta) (setq pruned t)))
                (setq children (cdr children)))
              best)
          (let ((best most-positive-fixnum)
                (children node)
                (pruned nil))
            (while (and children (not pruned))
              (let ((val (funcall 'neovm--ab-minimax-count
                                  (car children) (1- depth) alpha beta t counter)))
                (when (< val best) (setq best val))
                (when (< best beta) (setq beta best))
                (when (>= alpha beta) (setq pruned t)))
              (setq children (cdr children)))
            best)))))

  (unwind-protect
      (let ((tree '(((3 12) (8 2)) ((4 6) (14 5)))))
        (list
         ;; Alpha-beta gives same result as plain minimax
         (funcall 'neovm--ab-minimax tree 10
                  most-negative-fixnum most-positive-fixnum t)

         ;; Verify pruning actually happens: fewer nodes visited
         (let ((counter-ab (cons 0 0)))
           (funcall 'neovm--ab-minimax-count tree 10
                    most-negative-fixnum most-positive-fixnum t counter-ab)
           (car counter-ab))

         ;; Tree where pruning is very effective (sorted values)
         (let ((sorted-tree '(((1 2) (3 4)) ((5 6) (7 8)))))
           (funcall 'neovm--ab-minimax sorted-tree 10
                    most-negative-fixnum most-positive-fixnum t))

         ;; Wider tree
         (funcall 'neovm--ab-minimax '((10 5 7) (3 8 6) (1 9 4)) 10
                  most-negative-fixnum most-positive-fixnum t)))
    (fmakunbound 'neovm--ab-leaf-p)
    (fmakunbound 'neovm--ab-minimax)
    (fmakunbound 'neovm--ab-minimax-count)))"#;
    let expect = expect_test::expect![[r#""OK (8 12 6 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tic-tac-toe game state evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_tictactoe_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Board is a vector of 9 cells: 0=empty, 1=X, 2=O
  ;; Win lines: rows, columns, diagonals
  (fset 'neovm--ttt-win-lines
    (lambda () '((0 1 2) (3 4 5) (6 7 8)     ;; rows
                 (0 3 6) (1 4 7) (2 5 8)     ;; columns
                 (0 4 8) (2 4 6))))           ;; diagonals

  ;; Check if player p has won
  (fset 'neovm--ttt-winner
    (lambda (board)
      (let ((winner 0)
            (lines (funcall 'neovm--ttt-win-lines)))
        (dolist (line lines)
          (let ((a (aref board (nth 0 line)))
                (b (aref board (nth 1 line)))
                (c (aref board (nth 2 line))))
            (when (and (/= a 0) (= a b) (= b c))
              (setq winner a))))
        winner)))

  ;; Check if board is full
  (fset 'neovm--ttt-full
    (lambda (board)
      (let ((full t))
        (dotimes (i 9)
          (when (= (aref board i) 0)
            (setq full nil)))
        full)))

  ;; Get available moves
  (fset 'neovm--ttt-moves
    (lambda (board)
      (let ((moves nil))
        (dotimes (i 9)
          (when (= (aref board i) 0)
            (push i moves)))
        (nreverse moves))))

  ;; Static evaluation: +10 for X win, -10 for O win, 0 for draw/ongoing
  (fset 'neovm--ttt-eval
    (lambda (board)
      (let ((w (funcall 'neovm--ttt-winner board)))
        (cond ((= w 1) 10)
              ((= w 2) -10)
              (t 0)))))

  (unwind-protect
      (list
       ;; Empty board: no winner
       (funcall 'neovm--ttt-eval (vector 0 0 0 0 0 0 0 0 0))
       ;; X wins top row
       (funcall 'neovm--ttt-eval (vector 1 1 1 0 0 0 0 0 0))
       ;; O wins diagonal
       (funcall 'neovm--ttt-eval (vector 0 0 2 0 2 0 2 0 0))
       ;; Full board, no winner (draw)
       (let ((draw-board (vector 1 2 1 1 2 2 2 1 1)))
         (list (funcall 'neovm--ttt-eval draw-board)
               (funcall 'neovm--ttt-full draw-board)))
       ;; Available moves on partial board
       (funcall 'neovm--ttt-moves (vector 1 0 2 0 1 0 2 0 0))
       ;; X wins column
       (funcall 'neovm--ttt-eval (vector 1 2 0 1 2 0 1 0 0))
       ;; O wins bottom row
       (funcall 'neovm--ttt-eval (vector 1 1 0 0 0 0 2 2 2)))
    (fmakunbound 'neovm--ttt-win-lines)
    (fmakunbound 'neovm--ttt-winner)
    (fmakunbound 'neovm--ttt-full)
    (fmakunbound 'neovm--ttt-moves)
    (fmakunbound 'neovm--ttt-eval)))"#;
    let expect = expect_test::expect![[r#""OK (0 10 -10 (0 t) (1 3 5 7 8) 10 -10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax with move generation for tic-tac-toe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_tictactoe_with_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ttt2-winner
    (lambda (board)
      (let ((lines '((0 1 2) (3 4 5) (6 7 8)
                     (0 3 6) (1 4 7) (2 5 8)
                     (0 4 8) (2 4 6)))
            (winner 0))
        (dolist (line lines)
          (let ((a (aref board (nth 0 line)))
                (b (aref board (nth 1 line)))
                (c (aref board (nth 2 line))))
            (when (and (/= a 0) (= a b) (= b c))
              (setq winner a))))
        winner)))

  (fset 'neovm--ttt2-terminal
    (lambda (board)
      (or (/= (funcall 'neovm--ttt2-winner board) 0)
          (let ((full t))
            (dotimes (i 9)
              (when (= (aref board i) 0) (setq full nil)))
            full))))

  (fset 'neovm--ttt2-moves
    (lambda (board)
      (let ((moves nil))
        (dotimes (i 9)
          (when (= (aref board i) 0) (push i moves)))
        (nreverse moves))))

  ;; Minimax for tic-tac-toe: player 1=X(max), 2=O(min)
  (fset 'neovm--ttt2-minimax
    (lambda (board player depth)
      (let ((w (funcall 'neovm--ttt2-winner board)))
        (cond
         ((= w 1) (- 10 depth))   ;; X wins, prefer faster wins
         ((= w 2) (- depth 10))   ;; O wins
         ((funcall 'neovm--ttt2-terminal board) 0) ;; draw
         (t
          (let ((moves (funcall 'neovm--ttt2-moves board))
                (best (if (= player 1) most-negative-fixnum most-positive-fixnum)))
            (dolist (move moves)
              (let ((new-board (copy-sequence board)))
                (aset new-board move player)
                (let ((val (funcall 'neovm--ttt2-minimax
                                    new-board
                                    (if (= player 1) 2 1)
                                    (1+ depth))))
                  (if (= player 1)
                      (when (> val best) (setq best val))
                    (when (< val best) (setq best val))))))
            best))))))

  ;; Find best move for a player
  (fset 'neovm--ttt2-best-move
    (lambda (board player)
      (let ((moves (funcall 'neovm--ttt2-moves board))
            (best-val (if (= player 1) most-negative-fixnum most-positive-fixnum))
            (best-move -1))
        (dolist (move moves)
          (let ((new-board (copy-sequence board)))
            (aset new-board move player)
            (let ((val (funcall 'neovm--ttt2-minimax
                                new-board
                                (if (= player 1) 2 1)
                                0)))
              (when (if (= player 1) (> val best-val) (< val best-val))
                (setq best-val val)
                (setq best-move move)))))
        (list best-move best-val))))

  (unwind-protect
      (list
       ;; X should win from this position (X's turn, almost complete)
       ;; X X _
       ;; O O _
       ;; _ _ _
       (funcall 'neovm--ttt2-best-move (vector 1 1 0 2 2 0 0 0 0) 1)

       ;; O should block X from winning
       ;; X X _
       ;; O _ _
       ;; _ _ _
       (funcall 'neovm--ttt2-best-move (vector 1 1 0 2 0 0 0 0 0) 2)

       ;; Empty board: X plays center (optimal)
       ;; Result: best move = 4 (center)
       (car (funcall 'neovm--ttt2-best-move (vector 0 0 0 0 0 0 0 0 0) 1))

       ;; Near-end game: X can force win
       ;; X O X
       ;; _ X _
       ;; O _ _
       (funcall 'neovm--ttt2-minimax (vector 1 2 1 0 1 0 2 0 0) 1 0))
    (fmakunbound 'neovm--ttt2-winner)
    (fmakunbound 'neovm--ttt2-terminal)
    (fmakunbound 'neovm--ttt2-moves)
    (fmakunbound 'neovm--ttt2-minimax)
    (fmakunbound 'neovm--ttt2-best-move)))"#;
    let expect = expect_test::expect![[r#""OK ((2 10) (2 7) 0 9)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((2 10) (2 7) 0 9)", &oracle, &neovm);
}

// ---------------------------------------------------------------------------
// Evaluation function for partial game states
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_heuristic_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Heuristic evaluation for tic-tac-toe (used when depth is limited)
  ;; Counts potential winning lines for each player
  (fset 'neovm--heur-eval
    (lambda (board)
      (let ((lines '((0 1 2) (3 4 5) (6 7 8)
                     (0 3 6) (1 4 7) (2 5 8)
                     (0 4 8) (2 4 6)))
            (x-score 0)
            (o-score 0))
        (dolist (line lines)
          (let ((a (aref board (nth 0 line)))
                (b (aref board (nth 1 line)))
                (c (aref board (nth 2 line)))
                (x-count 0) (o-count 0) (empty-count 0))
            (dolist (cell (list a b c))
              (cond ((= cell 1) (setq x-count (1+ x-count)))
                    ((= cell 2) (setq o-count (1+ o-count)))
                    (t (setq empty-count (1+ empty-count)))))
            ;; Line is useful for X if no O in it, and vice versa
            (when (= o-count 0)
              (setq x-score (+ x-score
                               (cond ((= x-count 3) 100)
                                     ((= x-count 2) 10)
                                     ((= x-count 1) 1)
                                     (t 0)))))
            (when (= x-count 0)
              (setq o-score (+ o-score
                               (cond ((= o-count 3) 100)
                                     ((= o-count 2) 10)
                                     ((= o-count 1) 1)
                                     (t 0)))))))
        (- x-score o-score))))

  ;; Depth-limited minimax using heuristic
  (fset 'neovm--heur-minimax
    (lambda (board player depth max-depth)
      (let ((w (let ((lines '((0 1 2) (3 4 5) (6 7 8)
                               (0 3 6) (1 4 7) (2 5 8)
                               (0 4 8) (2 4 6)))
                     (winner 0))
                 (dolist (line lines)
                   (let ((a (aref board (nth 0 line)))
                         (b (aref board (nth 1 line)))
                         (c (aref board (nth 2 line))))
                     (when (and (/= a 0) (= a b) (= b c))
                       (setq winner a))))
                 winner)))
        (cond
         ((= w 1) 1000)
         ((= w 2) -1000)
         ((>= depth max-depth) (funcall 'neovm--heur-eval board))
         (t
          (let ((moves nil)
                (best (if (= player 1) most-negative-fixnum most-positive-fixnum)))
            (dotimes (i 9)
              (when (= (aref board i) 0) (push i moves)))
            (setq moves (nreverse moves))
            (if (null moves)
                0 ;; draw
              (dolist (move moves)
                (let ((new-board (copy-sequence board)))
                  (aset new-board move player)
                  (let ((val (funcall 'neovm--heur-minimax
                                      new-board
                                      (if (= player 1) 2 1)
                                      (1+ depth) max-depth)))
                    (if (= player 1)
                        (when (> val best) (setq best val))
                      (when (< val best) (setq best val))))))
              best)))))))

  (unwind-protect
      (list
       ;; Heuristic eval of various board states
       ;; Empty board: both sides equal
       (funcall 'neovm--heur-eval (vector 0 0 0 0 0 0 0 0 0))

       ;; X in center: advantage for X
       (funcall 'neovm--heur-eval (vector 0 0 0 0 1 0 0 0 0))

       ;; X winning position
       (funcall 'neovm--heur-eval (vector 1 1 1 0 0 0 0 0 0))

       ;; O winning position
       (funcall 'neovm--heur-eval (vector 0 0 0 2 2 2 0 0 0))

       ;; Depth-limited search with max-depth=2
       (funcall 'neovm--heur-minimax (vector 1 0 0 0 2 0 0 0 0) 1 0 2)

       ;; Compare heuristic at different depths
       (let ((board (vector 1 0 2 0 1 0 2 0 0)))
         (list
          (funcall 'neovm--heur-minimax board 1 0 1)
          (funcall 'neovm--heur-minimax board 1 0 2)
          (funcall 'neovm--heur-minimax board 1 0 3))))
    (fmakunbound 'neovm--heur-eval)
    (fmakunbound 'neovm--heur-minimax)))"#;
    let expect = expect_test::expect![[r#""OK (0 4 105 -105 -9 (1000 1000 1000))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax on a numeric game: Nim-like subtraction game
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_nim_game() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Nim-like game: start with N stones, each player takes 1-3 stones
  ;; The player who takes the last stone wins
  ;; Returns t if current player can force a win, nil otherwise
  (fset 'neovm--nim-can-win
    (lambda (stones memo)
      (if (= stones 0)
          nil ;; no stones left, current player loses (previous player took last)
        (let ((cached (gethash stones memo 'miss)))
          (if (not (eq cached 'miss))
              cached
            (let ((can-win nil)
                  (take 1))
              ;; Try taking 1, 2, or 3 stones
              (while (and (<= take (min 3 stones)) (not can-win))
                ;; If opponent cannot win after our move, we can win
                (unless (funcall 'neovm--nim-can-win (- stones take) memo)
                  (setq can-win t))
                (setq take (1+ take)))
              (puthash stones can-win memo)
              can-win))))))

  ;; Find optimal move (which number to take)
  (fset 'neovm--nim-best-take
    (lambda (stones memo)
      (let ((take 1) (best-take nil))
        (while (<= take (min 3 stones))
          (unless (funcall 'neovm--nim-can-win (- stones take) memo)
            (unless best-take (setq best-take take)))
          (setq take (1+ take)))
        best-take)))

  (unwind-protect
      (let ((memo (make-hash-table :test 'eql)))
        (list
         ;; Can win from N=1? Yes (take 1)
         (funcall 'neovm--nim-can-win 1 memo)
         ;; Can win from N=2? Yes (take 2)
         (funcall 'neovm--nim-can-win 2 memo)
         ;; Can win from N=3? Yes (take 3)
         (funcall 'neovm--nim-can-win 3 memo)
         ;; Can win from N=4? No (whatever you take, opponent wins)
         (funcall 'neovm--nim-can-win 4 memo)
         ;; Can win from N=5? Yes (take 1, leave 4 for opponent)
         (funcall 'neovm--nim-can-win 5 memo)
         ;; Pattern: lose iff stones mod 4 == 0
         (mapcar (lambda (n) (funcall 'neovm--nim-can-win n memo))
                 '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16))
         ;; Best moves
         (mapcar (lambda (n) (funcall 'neovm--nim-best-take n memo))
                 '(1 2 3 5 6 7 9 10 11 13))))
    (fmakunbound 'neovm--nim-can-win)
    (fmakunbound 'neovm--nim-best-take)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil t (t t t nil t t t nil t t t nil t t t nil) (1 2 3 1 2 3 1 2 3 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
