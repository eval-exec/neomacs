//! Advanced oracle parity tests for minimax with alpha-beta pruning:
//! game tree representation, minimax evaluation, alpha-beta pruning,
//! tic-tac-toe AI, move ordering, iterative deepening.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Game tree representation and traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_game_tree_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Game tree nodes: (:leaf val) or (:node children)
    // Build, traverse, count, and evaluate game trees.
    let form = r#"(progn
  (fset 'neovm--mma-leaf (lambda (val) (list :leaf val)))
  (fset 'neovm--mma-node (lambda (&rest children) (cons :node children)))
  (fset 'neovm--mma-leaf-p (lambda (n) (eq (car n) :leaf)))
  (fset 'neovm--mma-leaf-val (lambda (n) (cadr n)))
  (fset 'neovm--mma-children (lambda (n) (cdr n)))

  ;; Count total nodes (leaves + internal)
  (fset 'neovm--mma-count-nodes
    (lambda (tree)
      (if (funcall 'neovm--mma-leaf-p tree) 1
        (let ((count 1))
          (dolist (ch (funcall 'neovm--mma-children tree))
            (setq count (+ count (funcall 'neovm--mma-count-nodes ch))))
          count))))

  ;; Count leaves only
  (fset 'neovm--mma-count-leaves
    (lambda (tree)
      (if (funcall 'neovm--mma-leaf-p tree) 1
        (let ((count 0))
          (dolist (ch (funcall 'neovm--mma-children tree))
            (setq count (+ count (funcall 'neovm--mma-count-leaves ch))))
          count))))

  ;; Depth of tree
  (fset 'neovm--mma-depth
    (lambda (tree)
      (if (funcall 'neovm--mma-leaf-p tree) 0
        (let ((max-d 0))
          (dolist (ch (funcall 'neovm--mma-children tree))
            (let ((d (funcall 'neovm--mma-depth ch)))
              (when (> d max-d) (setq max-d d))))
          (1+ max-d)))))

  ;; Collect all leaf values left-to-right
  (fset 'neovm--mma-leaf-values
    (lambda (tree)
      (if (funcall 'neovm--mma-leaf-p tree)
          (list (funcall 'neovm--mma-leaf-val tree))
        (let ((vals nil))
          (dolist (ch (funcall 'neovm--mma-children tree))
            (setq vals (append vals (funcall 'neovm--mma-leaf-values ch))))
          vals))))

  (unwind-protect
      (let* ((t1 (funcall 'neovm--mma-node
                           (funcall 'neovm--mma-node
                                    (funcall 'neovm--mma-leaf 3)
                                    (funcall 'neovm--mma-leaf 5))
                           (funcall 'neovm--mma-node
                                    (funcall 'neovm--mma-leaf 2)
                                    (funcall 'neovm--mma-leaf 9))))
             ;; Wider tree with 3 branches per node
             (t2 (funcall 'neovm--mma-node
                           (funcall 'neovm--mma-node
                                    (funcall 'neovm--mma-leaf 1)
                                    (funcall 'neovm--mma-leaf 4)
                                    (funcall 'neovm--mma-leaf 7))
                           (funcall 'neovm--mma-node
                                    (funcall 'neovm--mma-leaf 2)
                                    (funcall 'neovm--mma-leaf 5))
                           (funcall 'neovm--mma-leaf 8)))
             ;; Deep tree
             (t3 (funcall 'neovm--mma-node
                           (funcall 'neovm--mma-node
                                    (funcall 'neovm--mma-node
                                             (funcall 'neovm--mma-leaf 10)
                                             (funcall 'neovm--mma-leaf 20))
                                    (funcall 'neovm--mma-leaf 30))
                           (funcall 'neovm--mma-leaf 40))))
        (list
         ;; Node counts
         (funcall 'neovm--mma-count-nodes t1)
         (funcall 'neovm--mma-count-nodes t2)
         (funcall 'neovm--mma-count-nodes t3)
         ;; Leaf counts
         (funcall 'neovm--mma-count-leaves t1)
         (funcall 'neovm--mma-count-leaves t2)
         (funcall 'neovm--mma-count-leaves t3)
         ;; Depths
         (funcall 'neovm--mma-depth t1)
         (funcall 'neovm--mma-depth t2)
         (funcall 'neovm--mma-depth t3)
         ;; Leaf values
         (funcall 'neovm--mma-leaf-values t1)
         (funcall 'neovm--mma-leaf-values t2)
         (funcall 'neovm--mma-leaf-values t3)
         ;; Single leaf tree
         (funcall 'neovm--mma-count-nodes (funcall 'neovm--mma-leaf 42))
         (funcall 'neovm--mma-depth (funcall 'neovm--mma-leaf 42))))
    (fmakunbound 'neovm--mma-leaf)
    (fmakunbound 'neovm--mma-node)
    (fmakunbound 'neovm--mma-leaf-p)
    (fmakunbound 'neovm--mma-leaf-val)
    (fmakunbound 'neovm--mma-children)
    (fmakunbound 'neovm--mma-count-nodes)
    (fmakunbound 'neovm--mma-count-leaves)
    (fmakunbound 'neovm--mma-depth)
    (fmakunbound 'neovm--mma-leaf-values)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 9 7 4 6 4 2 2 3 (3 5 2 9) (1 4 7 2 5 8) (10 20 30 40) 1 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax with alpha-beta: pruning count and correctness
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_alpha_beta_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Plain minimax with node counter
  (fset 'neovm--mma-mm-plain
    (lambda (tree depth maximizing counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree 0)
        (if maximizing
            (let ((best most-negative-fixnum))
              (dolist (child tree)
                (let ((val (funcall 'neovm--mma-mm-plain child (1- depth) nil counter)))
                  (when (> val best) (setq best val))))
              best)
          (let ((best most-positive-fixnum))
            (dolist (child tree)
              (let ((val (funcall 'neovm--mma-mm-plain child (1- depth) t counter)))
                (when (< val best) (setq best val))))
            best)))))

  ;; Alpha-beta with node counter
  (fset 'neovm--mma-mm-ab
    (lambda (tree depth alpha beta maximizing counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree 0)
        (if maximizing
            (let ((best most-negative-fixnum)
                  (children tree) (pruned nil))
              (while (and children (not pruned))
                (let ((val (funcall 'neovm--mma-mm-ab
                                    (car children) (1- depth) alpha beta nil counter)))
                  (when (> val best) (setq best val))
                  (when (> best alpha) (setq alpha best))
                  (when (>= alpha beta) (setq pruned t)))
                (setq children (cdr children)))
              best)
          (let ((best most-positive-fixnum)
                (children tree) (pruned nil))
            (while (and children (not pruned))
              (let ((val (funcall 'neovm--mma-mm-ab
                                  (car children) (1- depth) alpha beta t counter)))
                (when (< val best) (setq best val))
                (when (< best beta) (setq beta best))
                (when (>= alpha beta) (setq pruned t)))
              (setq children (cdr children)))
            best)))))

  (unwind-protect
      (let* ((tree1 '(((3 12) (8 2)) ((4 6) (14 5))))
             ;; Larger tree for more visible pruning
             (tree2 '(((3 17) (2 12)) ((15 25) (0 2)) ((7 14) (5 8)) ((1 9) (11 6))))
             ;; Best-case pruning tree: sorted ascending
             (tree3 '(((1 2) (3 4)) ((5 6) (7 8))))
             ;; Worst-case: reverse sorted
             (tree4 '(((8 7) (6 5)) ((4 3) (2 1)))))
        (list
         ;; tree1: both should give same value
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-mm-plain tree1 10 t c1))
                 (v2 (funcall 'neovm--mma-mm-ab tree1 10
                              most-negative-fixnum most-positive-fixnum t c2)))
             (list v1 v2 (= v1 v2)
                   (car c1) (car c2)
                   ;; Alpha-beta visits fewer or equal nodes
                   (<= (car c2) (car c1)))))

         ;; tree2: same result, fewer nodes
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-mm-plain tree2 10 t c1))
                 (v2 (funcall 'neovm--mma-mm-ab tree2 10
                              most-negative-fixnum most-positive-fixnum t c2)))
             (list v1 v2 (= v1 v2)
                   (car c1) (car c2)
                   (<= (car c2) (car c1)))))

         ;; tree3: sorted ascending (good for alpha-beta)
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (funcall 'neovm--mma-mm-plain tree3 10 t c1)
           (funcall 'neovm--mma-mm-ab tree3 10
                    most-negative-fixnum most-positive-fixnum t c2)
           (list (car c1) (car c2)))

         ;; tree4: reverse sorted (worst case for alpha-beta)
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (funcall 'neovm--mma-mm-plain tree4 10 t c1)
           (funcall 'neovm--mma-mm-ab tree4 10
                    most-negative-fixnum most-positive-fixnum t c2)
           (list (car c1) (car c2)))))
    (fmakunbound 'neovm--mma-mm-plain)
    (fmakunbound 'neovm--mma-mm-ab)))"#;
    let expect =
        expect_test::expect![[r#""OK ((8 8 t 15 12 t) (12 12 t 29 26 t) (15 13) (15 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tic-tac-toe AI: complete game play
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_ttt_full_game() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mma-ttt-winner
    (lambda (b)
      (let ((lines '((0 1 2) (3 4 5) (6 7 8)
                     (0 3 6) (1 4 7) (2 5 8)
                     (0 4 8) (2 4 6)))
            (w 0))
        (dolist (l lines)
          (let ((a (aref b (car l)))
                (b2 (aref b (cadr l)))
                (c (aref b (caddr l))))
            (when (and (/= a 0) (= a b2) (= b2 c))
              (setq w a))))
        w)))

  (fset 'neovm--mma-ttt-terminal
    (lambda (b)
      (or (/= (funcall 'neovm--mma-ttt-winner b) 0)
          (let ((full t))
            (dotimes (i 9)
              (when (= (aref b i) 0) (setq full nil)))
            full))))

  (fset 'neovm--mma-ttt-moves
    (lambda (b)
      (let ((m nil))
        (dotimes (i 9)
          (when (= (aref b i) 0) (push i m)))
        (nreverse m))))

  ;; Alpha-beta minimax for TTT
  (fset 'neovm--mma-ttt-ab
    (lambda (b player depth alpha beta)
      (let ((w (funcall 'neovm--mma-ttt-winner b)))
        (cond
         ((= w 1) (- 100 depth))
         ((= w 2) (- depth 100))
         ((funcall 'neovm--mma-ttt-terminal b) 0)
         (t
          (let ((moves (funcall 'neovm--mma-ttt-moves b))
                (best (if (= player 1) most-negative-fixnum most-positive-fixnum))
                (pruned nil))
            (while (and moves (not pruned))
              (let ((nb (copy-sequence b)))
                (aset nb (car moves) player)
                (let ((val (funcall 'neovm--mma-ttt-ab nb
                                    (if (= player 1) 2 1)
                                    (1+ depth) alpha beta)))
                  (if (= player 1)
                      (progn
                        (when (> val best) (setq best val))
                        (when (> best alpha) (setq alpha best)))
                    (progn
                      (when (< val best) (setq best val))
                      (when (< best beta) (setq beta best))))
                  (when (>= alpha beta) (setq pruned t))))
              (setq moves (cdr moves)))
            best))))))

  ;; Best move using alpha-beta
  (fset 'neovm--mma-ttt-best-move
    (lambda (b player)
      (let ((moves (funcall 'neovm--mma-ttt-moves b))
            (best-val (if (= player 1) most-negative-fixnum most-positive-fixnum))
            (best-move -1))
        (dolist (m moves)
          (let ((nb (copy-sequence b)))
            (aset nb m player)
            (let ((val (funcall 'neovm--mma-ttt-ab nb
                                (if (= player 1) 2 1)
                                0 most-negative-fixnum most-positive-fixnum)))
              (when (if (= player 1) (> val best-val) (< val best-val))
                (setq best-val val)
                (setq best-move m)))))
        (list best-move best-val))))

  ;; Play a full game: two perfect AIs should draw
  (fset 'neovm--mma-ttt-play-game
    (lambda ()
      (let ((b (vector 0 0 0 0 0 0 0 0 0))
            (player 1)
            (move-history nil))
        (while (not (funcall 'neovm--mma-ttt-terminal b))
          (let ((result (funcall 'neovm--mma-ttt-best-move b player)))
            (let ((move (car result)))
              (setq move-history (cons (list player move) move-history))
              (aset b move player)
              (setq player (if (= player 1) 2 1)))))
        (list (funcall 'neovm--mma-ttt-winner b)
              (length move-history)
              (nreverse move-history)))))

  (unwind-protect
      (let ((game (funcall 'neovm--mma-ttt-play-game)))
        (list
         ;; Two perfect players should draw (winner = 0)
         (car game)
         ;; Number of moves played
         (cadr game)
         ;; The game move history
         (caddr game)
         ;; Specific positions: X should take center first
         (car (funcall 'neovm--mma-ttt-best-move
                       (vector 0 0 0 0 0 0 0 0 0) 1))
         ;; X should complete win when possible
         ;; X X _
         ;; O O _
         ;; _ _ _
         (car (funcall 'neovm--mma-ttt-best-move
                       (vector 1 1 0 2 2 0 0 0 0) 1))
         ;; O should block X's winning line
         ;; X X _
         ;; O _ _
         ;; _ _ _
         (car (funcall 'neovm--mma-ttt-best-move
                       (vector 1 1 0 2 0 0 0 0 0) 2))))
    (fmakunbound 'neovm--mma-ttt-winner)
    (fmakunbound 'neovm--mma-ttt-terminal)
    (fmakunbound 'neovm--mma-ttt-moves)
    (fmakunbound 'neovm--mma-ttt-ab)
    (fmakunbound 'neovm--mma-ttt-best-move)
    (fmakunbound 'neovm--mma-ttt-play-game)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 9 ((1 0) (2 4) (1 1) (2 2) (1 6) (2 3) (1 5) (2 7) (1 8)) 0 2 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Move ordering for improved alpha-beta efficiency
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_move_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Alpha-beta with ordered children (best-first heuristic)
  ;; For maximizing: sort children descending by heuristic value
  ;; For minimizing: sort children ascending

  ;; Simple heuristic: if leaf, its value; if internal, average of leaves
  (fset 'neovm--mma-mo-heuristic
    (lambda (tree)
      (if (integerp tree) tree
        (let ((sum 0) (count 0))
          (dolist (ch tree)
            (setq sum (+ sum (funcall 'neovm--mma-mo-heuristic ch)))
            (setq count (1+ count)))
          (if (> count 0) (/ sum count) 0)))))

  ;; Sort children by heuristic
  (fset 'neovm--mma-mo-order
    (lambda (children maximizing)
      (let ((scored (mapcar (lambda (ch)
                              (cons (funcall 'neovm--mma-mo-heuristic ch) ch))
                            children)))
        (setq scored (sort scored (if maximizing
                                      (lambda (a b) (> (car a) (car b)))
                                    (lambda (a b) (< (car a) (car b))))))
        (mapcar #'cdr scored))))

  ;; Alpha-beta with move ordering and counter
  (fset 'neovm--mma-mo-ab
    (lambda (tree depth alpha beta maximizing counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree 0)
        (let* ((ordered (funcall 'neovm--mma-mo-order tree maximizing))
               (best (if maximizing most-negative-fixnum most-positive-fixnum))
               (pruned nil))
          (while (and ordered (not pruned))
            (let ((val (funcall 'neovm--mma-mo-ab
                                (car ordered) (1- depth) alpha beta
                                (not maximizing) counter)))
              (if maximizing
                  (progn
                    (when (> val best) (setq best val))
                    (when (> best alpha) (setq alpha best)))
                (progn
                  (when (< val best) (setq best val))
                  (when (< best beta) (setq beta best))))
              (when (>= alpha beta) (setq pruned t)))
            (setq ordered (cdr ordered)))
          best))))

  ;; Standard alpha-beta without ordering for comparison
  (fset 'neovm--mma-mo-ab-unordered
    (lambda (tree depth alpha beta maximizing counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree 0)
        (let ((best (if maximizing most-negative-fixnum most-positive-fixnum))
              (children tree)
              (pruned nil))
          (while (and children (not pruned))
            (let ((val (funcall 'neovm--mma-mo-ab-unordered
                                (car children) (1- depth) alpha beta
                                (not maximizing) counter)))
              (if maximizing
                  (progn
                    (when (> val best) (setq best val))
                    (when (> best alpha) (setq alpha best)))
                (progn
                  (when (< val best) (setq best val))
                  (when (< best beta) (setq beta best))))
              (when (>= alpha beta) (setq pruned t)))
            (setq children (cdr children)))
          best))))

  (unwind-protect
      (let ((tree '(((10 5 7) (3 8 6)) ((1 9 4) (12 2 11)))))
        (list
         ;; Both should give same value
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-mo-ab tree 10
                              most-negative-fixnum most-positive-fixnum t c1))
                 (v2 (funcall 'neovm--mma-mo-ab-unordered tree 10
                              most-negative-fixnum most-positive-fixnum t c2)))
             (list v1 v2 (= v1 v2)
                   (car c1) (car c2))))

         ;; Heuristic values for children
         (mapcar 'neovm--mma-mo-heuristic tree)

         ;; Ordered vs unordered on another tree
         (let ((tree2 '(((1 20) (15 3)) ((18 2) (7 12))))
               (c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-mo-ab tree2 10
                              most-negative-fixnum most-positive-fixnum t c1))
                 (v2 (funcall 'neovm--mma-mo-ab-unordered tree2 10
                              most-negative-fixnum most-positive-fixnum t c2)))
             (list v1 v2 (= v1 v2)
                   (car c1) (car c2))))))
    (fmakunbound 'neovm--mma-mo-heuristic)
    (fmakunbound 'neovm--mma-mo-order)
    (fmakunbound 'neovm--mma-mo-ab)
    (fmakunbound 'neovm--mma-mo-ab-unordered)))"#;
    let expect = expect_test::expect![[r#""OK ((9 9 t 15 17) (6 6) (15 15 t 11 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Iterative deepening depth-first search (IDDFS) minimax
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_iterative_deepening() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Depth-limited alpha-beta
  (fset 'neovm--mma-id-ab
    (lambda (tree depth alpha beta maximizing)
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree
            ;; At depth limit for non-leaf: use heuristic (average of immediate children)
            (let ((sum 0) (count 0))
              (dolist (ch tree)
                (when (integerp ch)
                  (setq sum (+ sum ch))
                  (setq count (1+ count))))
              (if (> count 0) (/ sum count) 0)))
        (if maximizing
            (let ((best most-negative-fixnum) (children tree) (pruned nil))
              (while (and children (not pruned))
                (let ((val (funcall 'neovm--mma-id-ab
                                    (car children) (1- depth) alpha beta nil)))
                  (when (> val best) (setq best val))
                  (when (> best alpha) (setq alpha best))
                  (when (>= alpha beta) (setq pruned t)))
                (setq children (cdr children)))
              best)
          (let ((best most-positive-fixnum) (children tree) (pruned nil))
            (while (and children (not pruned))
              (let ((val (funcall 'neovm--mma-id-ab
                                  (car children) (1- depth) alpha beta t)))
                (when (< val best) (setq best val))
                (when (< best beta) (setq beta best))
                (when (>= alpha beta) (setq pruned t)))
              (setq children (cdr children)))
            best)))))

  ;; Iterative deepening: search at depth 1, 2, ..., max-depth
  ;; Returns (depth . value) for each depth searched
  (fset 'neovm--mma-id-search
    (lambda (tree max-depth)
      (let ((results nil) (d 1))
        (while (<= d max-depth)
          (let ((val (funcall 'neovm--mma-id-ab tree d
                              most-negative-fixnum most-positive-fixnum t)))
            (setq results (cons (cons d val) results)))
          (setq d (1+ d)))
        (nreverse results))))

  (unwind-protect
      (let ((tree '(((3 12) (8 2)) ((4 6) (14 5)))))
        (list
         ;; At maximum depth, ID gives same result as full search
         (funcall 'neovm--mma-id-ab tree 10
                  most-negative-fixnum most-positive-fixnum t)

         ;; ID search at increasing depths: value converges to true minimax
         (funcall 'neovm--mma-id-search tree 4)

         ;; Larger tree: check convergence
         (let ((big-tree '(((1 9 5) (7 3 11)) ((13 2 8) (4 10 6)))))
           (funcall 'neovm--mma-id-search big-tree 4))

         ;; Single depth: just runs once
         (funcall 'neovm--mma-id-search '((3 5) (2 9)) 2)

         ;; Verify: depth=1 is shallow (just looks at children heuristics)
         (funcall 'neovm--mma-id-ab tree 1
                  most-negative-fixnum most-positive-fixnum t)
         ;; Verify: full depth gives exact value
         (funcall 'neovm--mma-id-ab tree 10
                  most-negative-fixnum most-positive-fixnum t)))
    (fmakunbound 'neovm--mma-id-ab)
    (fmakunbound 'neovm--mma-id-search)))"#;
    let expect = expect_test::expect![[
        r#""OK (8 ((1 . 0) (2 . 5) (3 . 8) (4 . 8)) ((1 . 0) (2 . 6) (3 . 10) (4 . 10)) ((1 . 5) (2 . 3)) 0 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Negamax variant (simplified minimax formulation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_negamax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Negamax: simplifies minimax by negating perspective at each level.
  ;; Value at any node is always from the perspective of the player to move.
  ;; Leaf values must alternate sign at each level.
  ;;
  ;; For compatibility with standard game trees where leaves are absolute
  ;; values, we pass a 'color' parameter: +1 for maximizer, -1 for minimizer.
  (fset 'neovm--mma-negamax
    (lambda (tree depth color counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) (* color tree) 0)
        (let ((best most-negative-fixnum))
          (dolist (child tree)
            (let ((val (- (funcall 'neovm--mma-negamax
                                   child (1- depth) (- color) counter))))
              (when (> val best) (setq best val))))
          best))))

  ;; Negamax with alpha-beta
  (fset 'neovm--mma-negamax-ab
    (lambda (tree depth alpha beta color counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) (* color tree) 0)
        (let ((best most-negative-fixnum)
              (children tree) (pruned nil))
          (while (and children (not pruned))
            (let ((val (- (funcall 'neovm--mma-negamax-ab
                                   (car children) (1- depth)
                                   (- beta) (- alpha) (- color) counter))))
              (when (> val best) (setq best val))
              (when (> best alpha) (setq alpha best))
              (when (>= alpha beta) (setq pruned t)))
            (setq children (cdr children)))
          best))))

  ;; Standard minimax for comparison
  (fset 'neovm--mma-std-mm
    (lambda (tree depth maximizing counter)
      (setcar counter (1+ (car counter)))
      (if (or (integerp tree) (= depth 0))
          (if (integerp tree) tree 0)
        (if maximizing
            (let ((best most-negative-fixnum))
              (dolist (child tree)
                (let ((val (funcall 'neovm--mma-std-mm child (1- depth) nil counter)))
                  (when (> val best) (setq best val))))
              best)
          (let ((best most-positive-fixnum))
            (dolist (child tree)
              (let ((val (funcall 'neovm--mma-std-mm child (1- depth) t counter)))
                (when (< val best) (setq best val))))
            best)))))

  (unwind-protect
      (let ((tree1 '(((3 12) (8 2)) ((4 6) (14 5))))
            (tree2 '((10 5 7) (3 8 6) (1 9 4))))
        (list
         ;; Negamax with color=1 should give same result as standard minimax(max=t)
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-std-mm tree1 10 t c1))
                 (v2 (funcall 'neovm--mma-negamax tree1 10 1 c2)))
             (list v1 v2 (= v1 v2))))

         ;; Negamax alpha-beta: same result, fewer nodes
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-negamax tree1 10 1 c1))
                 (v2 (funcall 'neovm--mma-negamax-ab tree1 10
                              most-negative-fixnum most-positive-fixnum 1 c2)))
             (list v1 v2 (= v1 v2)
                   (car c1) (car c2)
                   (<= (car c2) (car c1)))))

         ;; tree2: wider branching
         (let ((c1 (cons 0 0)) (c2 (cons 0 0)))
           (let ((v1 (funcall 'neovm--mma-std-mm tree2 10 t c1))
                 (v2 (funcall 'neovm--mma-negamax tree2 10 1 c2)))
             (list v1 v2 (= v1 v2))))

         ;; Negamax with color=-1 gives the minimizer's perspective
         (let ((c (cons 0 0)))
           (funcall 'neovm--mma-negamax tree1 10 -1 c))))
    (fmakunbound 'neovm--mma-negamax)
    (fmakunbound 'neovm--mma-negamax-ab)
    (fmakunbound 'neovm--mma-std-mm)))"#;
    let expect = expect_test::expect![[r#""OK ((8 8 t) (8 8 t 15 12 t) (5 5 t) -3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax on Connect-4 style column game (smaller: 4x4 board)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minimax_advanced_connect_game() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Small 4x3 "connect 3" game using minimax
    let form = r#"(progn
  ;; 4 columns, 3 rows. Board is a vector of 12 (row-major).
  ;; Index: row*4+col. 0=empty, 1=X, 2=O.
  ;; Win = 3 in a row (horizontal, vertical, or diagonal).

  (fset 'neovm--mma-c3-get
    (lambda (b row col) (aref b (+ (* row 4) col))))

  (fset 'neovm--mma-c3-set
    (lambda (b row col val)
      (let ((nb (copy-sequence b)))
        (aset nb (+ (* row 4) col) val)
        nb)))

  ;; Find the lowest empty row in a column (drop piece)
  (fset 'neovm--mma-c3-drop-row
    (lambda (b col)
      (let ((r 2) (found nil))
        (while (and (>= r 0) (not found))
          (if (= (funcall 'neovm--mma-c3-get b r col) 0)
              (setq found r)
            (setq r (1- r))))
        found)))

  ;; Check winner
  (fset 'neovm--mma-c3-winner
    (lambda (b)
      (let ((w 0))
        ;; Horizontal lines
        (dotimes (r 3)
          (dotimes (c 2)
            (let ((a (funcall 'neovm--mma-c3-get b r c))
                  (b2 (funcall 'neovm--mma-c3-get b r (1+ c)))
                  (c2 (funcall 'neovm--mma-c3-get b r (+ c 2))))
              (when (and (/= a 0) (= a b2) (= b2 c2))
                (setq w a)))))
        ;; Vertical lines
        (dotimes (c 4)
          (let ((a (funcall 'neovm--mma-c3-get b 0 c))
                (b2 (funcall 'neovm--mma-c3-get b 1 c))
                (c2 (funcall 'neovm--mma-c3-get b 2 c)))
            (when (and (/= a 0) (= a b2) (= b2 c2))
              (setq w a))))
        ;; Diagonals (down-right)
        (dotimes (c 2)
          (let ((a (funcall 'neovm--mma-c3-get b 0 c))
                (b2 (funcall 'neovm--mma-c3-get b 1 (1+ c)))
                (c2 (funcall 'neovm--mma-c3-get b 2 (+ c 2))))
            (when (and (/= a 0) (= a b2) (= b2 c2))
              (setq w a))))
        ;; Diagonals (down-left)
        (dotimes (c 2)
          (let ((cc (+ c 2)))
            (let ((a (funcall 'neovm--mma-c3-get b 0 cc))
                  (b2 (funcall 'neovm--mma-c3-get b 1 (1- cc)))
                  (c2 (funcall 'neovm--mma-c3-get b 2 (- cc 2))))
              (when (and (/= a 0) (= a b2) (= b2 c2))
                (setq w a)))))
        w)))

  (fset 'neovm--mma-c3-full
    (lambda (b)
      (let ((full t))
        (dotimes (i 12)
          (when (= (aref b i) 0) (setq full nil)))
        full)))

  ;; Available columns (where a piece can be dropped)
  (fset 'neovm--mma-c3-moves
    (lambda (b)
      (let ((m nil))
        (dotimes (c 4)
          (when (funcall 'neovm--mma-c3-drop-row b c)
            (push c m)))
        (nreverse m))))

  ;; Minimax for connect-3
  (fset 'neovm--mma-c3-minimax
    (lambda (b player depth)
      (let ((w (funcall 'neovm--mma-c3-winner b)))
        (cond
         ((= w 1) (- 50 depth))
         ((= w 2) (- depth 50))
         ((or (funcall 'neovm--mma-c3-full b) (= depth 8)) 0)
         (t
          (let ((moves (funcall 'neovm--mma-c3-moves b))
                (best (if (= player 1) most-negative-fixnum most-positive-fixnum)))
            (dolist (col moves)
              (let* ((row (funcall 'neovm--mma-c3-drop-row b col))
                     (nb (funcall 'neovm--mma-c3-set b row col player))
                     (val (funcall 'neovm--mma-c3-minimax nb
                                   (if (= player 1) 2 1) (1+ depth))))
                (if (= player 1)
                    (when (> val best) (setq best val))
                  (when (< val best) (setq best val)))))
            best))))))

  (fset 'neovm--mma-c3-best-col
    (lambda (b player)
      (let ((moves (funcall 'neovm--mma-c3-moves b))
            (best-val (if (= player 1) most-negative-fixnum most-positive-fixnum))
            (best-col -1))
        (dolist (col moves)
          (let* ((row (funcall 'neovm--mma-c3-drop-row b col))
                 (nb (funcall 'neovm--mma-c3-set b row col player))
                 (val (funcall 'neovm--mma-c3-minimax nb
                               (if (= player 1) 2 1) 0)))
            (when (if (= player 1) (> val best-val) (< val best-val))
              (setq best-val val)
              (setq best-col col))))
        (list best-col best-val))))

  (unwind-protect
      (let ((empty (vector 0 0 0 0  0 0 0 0  0 0 0 0)))
        (list
         ;; Winner detection
         (funcall 'neovm--mma-c3-winner empty)    ;; 0
         (funcall 'neovm--mma-c3-winner
                  (vector 1 1 1 0  0 0 0 0  0 0 0 0))  ;; 1 (horizontal top)
         (funcall 'neovm--mma-c3-winner
                  (vector 0 2 0 0  0 2 0 0  0 2 0 0))  ;; 2 (vertical col 1)
         ;; Available moves on empty board
         (funcall 'neovm--mma-c3-moves empty)
         ;; Best move from empty board
         (funcall 'neovm--mma-c3-best-col empty 1)
         ;; X should complete a win: X X _ _ / _ _ _ _ / _ _ _ _ (play col 2)
         (car (funcall 'neovm--mma-c3-best-col
                       (vector 0 0 0 0  0 0 0 0  1 1 0 0) 1))))
    (fmakunbound 'neovm--mma-c3-get)
    (fmakunbound 'neovm--mma-c3-set)
    (fmakunbound 'neovm--mma-c3-drop-row)
    (fmakunbound 'neovm--mma-c3-winner)
    (fmakunbound 'neovm--mma-c3-full)
    (fmakunbound 'neovm--mma-c3-moves)
    (fmakunbound 'neovm--mma-c3-minimax)
    (fmakunbound 'neovm--mma-c3-best-col)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 (0 1 2 3) (1 42) 2)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 1 2 (0 1 2 3) (1 42) 2)", &oracle, &neovm);
}
