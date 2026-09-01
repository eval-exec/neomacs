//! Oracle parity tests implementing graph coloring algorithms in Elisp:
//! greedy coloring, chromatic number estimation, k-colorability check,
//! register allocation as graph coloring, and Sudoku as graph coloring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Greedy graph coloring algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_greedy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Greedy coloring: process nodes in order, assign smallest color
    // not used by any neighbor.
    let form = r#"(progn
  (fset 'neovm--gc-greedy-color
    (lambda (adj-list nodes)
      "Greedy-color a graph. ADJ-LIST is hash-table of node -> neighbor list.
       Returns hash-table of node -> color (integer starting at 0)."
      (let ((colors (make-hash-table)))
        (dolist (node nodes)
          (let ((neighbor-colors (make-hash-table))
                (neighbors (gethash node adj-list nil)))
            ;; Collect colors used by neighbors
            (dolist (nbr neighbors)
              (let ((c (gethash nbr colors)))
                (when c (puthash c t neighbor-colors))))
            ;; Find smallest color not in neighbor-colors
            (let ((c 0))
              (while (gethash c neighbor-colors)
                (setq c (1+ c)))
              (puthash node c colors))))
        colors)))

  (unwind-protect
      (let ((g (make-hash-table)))
        ;; Build a pentagon graph (cycle of 5)
        (puthash 'a '(b e) g)
        (puthash 'b '(a c) g)
        (puthash 'c '(b d) g)
        (puthash 'd '(c e) g)
        (puthash 'e '(d a) g)
        (let ((colors (funcall 'neovm--gc-greedy-color g '(a b c d e))))
          ;; Verify: no two adjacent nodes have the same color
          (let ((valid t))
            (dolist (node '(a b c d e))
              (let ((my-color (gethash node colors)))
                (dolist (nbr (gethash node g))
                  (when (= my-color (gethash nbr colors))
                    (setq valid nil)))))
            ;; Collect color assignments (sorted by node name)
            (let ((assignments nil))
              (dolist (node '(a b c d e))
                (setq assignments
                      (cons (cons node (gethash node colors)) assignments)))
              (list
                'valid valid
                'assignments (nreverse assignments)
                'num-colors (let ((max-c 0))
                              (dolist (node '(a b c d e))
                                (setq max-c (max max-c (gethash node colors))))
                              (1+ max-c)))))))
    (fmakunbound 'neovm--gc-greedy-color)))"#;
    let expect = expect_test::expect![[
        r#""OK (valid t assignments ((a . 0) (b . 1) (c . 0) (d . 1) (e . 2)) num-colors 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Check if graph is k-colorable (backtracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_k_colorable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--gc-is-safe
    (lambda (adj-list node color colors)
      "Check if assigning COLOR to NODE is safe (no neighbor has same color)."
      (let ((safe t)
            (neighbors (gethash node adj-list nil)))
        (dolist (nbr neighbors)
          (when (and (gethash nbr colors) (= (gethash nbr colors) color))
            (setq safe nil)))
        safe)))

  (fset 'neovm--gc-backtrack
    (lambda (adj-list nodes k colors idx)
      "Try to k-color graph starting at node index IDX. Returns t if possible."
      (if (= idx (length nodes))
          t  ;; All nodes colored
        (let ((node (nth idx nodes))
              (found nil))
          (let ((c 0))
            (while (and (< c k) (not found))
              (when (funcall 'neovm--gc-is-safe adj-list node c colors)
                (puthash node c colors)
                (if (funcall 'neovm--gc-backtrack adj-list nodes k colors (1+ idx))
                    (setq found t)
                  (remhash node colors)))
              (setq c (1+ c))))
          found))))

  (fset 'neovm--gc-k-colorable
    (lambda (adj-list nodes k)
      "Check if graph is k-colorable. Returns (t . colors-alist) or (nil . nil)."
      (let ((colors (make-hash-table)))
        (if (funcall 'neovm--gc-backtrack adj-list nodes k colors 0)
            (let ((result nil))
              (dolist (n nodes)
                (setq result (cons (cons n (gethash n colors)) result)))
              (cons t (nreverse result)))
          (cons nil nil)))))

  (unwind-protect
      (let ((triangle (make-hash-table))
            (square (make-hash-table))
            (k4 (make-hash-table)))
        ;; Triangle (3-clique): needs 3 colors
        (puthash 'a '(b c) triangle)
        (puthash 'b '(a c) triangle)
        (puthash 'c '(a b) triangle)
        ;; Square (cycle of 4): needs 2 colors
        (puthash 'p '(q s) square)
        (puthash 'q '(p r) square)
        (puthash 'r '(q s) square)
        (puthash 's '(r p) square)
        ;; K4 (complete graph on 4): needs 4 colors
        (puthash 1 '(2 3 4) k4)
        (puthash 2 '(1 3 4) k4)
        (puthash 3 '(1 2 4) k4)
        (puthash 4 '(1 2 3) k4)
        (list
          ;; Triangle: 2-colorable? No. 3-colorable? Yes.
          (car (funcall 'neovm--gc-k-colorable triangle '(a b c) 2))
          (car (funcall 'neovm--gc-k-colorable triangle '(a b c) 3))
          ;; Square: 2-colorable? Yes. 1-colorable? No.
          (car (funcall 'neovm--gc-k-colorable square '(p q r s) 2))
          (car (funcall 'neovm--gc-k-colorable square '(p q r s) 1))
          ;; K4: 3-colorable? No. 4-colorable? Yes.
          (car (funcall 'neovm--gc-k-colorable k4 '(1 2 3 4) 3))
          (car (funcall 'neovm--gc-k-colorable k4 '(1 2 3 4) 4))
          ;; Get actual coloring for square with 2 colors
          (funcall 'neovm--gc-k-colorable square '(p q r s) 2)))
    (fmakunbound 'neovm--gc-is-safe)
    (fmakunbound 'neovm--gc-backtrack)
    (fmakunbound 'neovm--gc-k-colorable)))"#;
    let expect =
        expect_test::expect![[r#""OK (nil t t nil nil t (t (p . 0) (q . 1) (r . 0) (s . 1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chromatic number estimation (find minimum k)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_chromatic_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--gc-is-safe
    (lambda (adj-list node color colors)
      (let ((safe t))
        (dolist (nbr (gethash node adj-list nil))
          (when (and (gethash nbr colors) (= (gethash nbr colors) color))
            (setq safe nil)))
        safe)))

  (fset 'neovm--gc-backtrack
    (lambda (adj-list nodes k colors idx)
      (if (= idx (length nodes)) t
        (let ((node (nth idx nodes)) (found nil))
          (let ((c 0))
            (while (and (< c k) (not found))
              (when (funcall 'neovm--gc-is-safe adj-list node c colors)
                (puthash node c colors)
                (if (funcall 'neovm--gc-backtrack adj-list nodes k colors (1+ idx))
                    (setq found t)
                  (remhash node colors)))
              (setq c (1+ c))))
          found))))

  (fset 'neovm--gc-chromatic-number
    (lambda (adj-list nodes)
      "Find the chromatic number: minimum k such that graph is k-colorable."
      (let ((k 1)
            (n (length nodes))
            (found nil))
        (while (and (<= k n) (not found))
          (let ((colors (make-hash-table)))
            (if (funcall 'neovm--gc-backtrack adj-list nodes k colors 0)
                (setq found t)
              (setq k (1+ k)))))
        k)))

  (unwind-protect
      (let ((empty (make-hash-table))
            (edge (make-hash-table))
            (triangle (make-hash-table))
            (bipartite (make-hash-table))
            (petersen-like (make-hash-table)))
        ;; Single node: chromatic number = 1
        (puthash 'x nil empty)
        ;; Single edge: 2
        (puthash 'a '(b) edge)
        (puthash 'b '(a) edge)
        ;; Triangle: 3
        (puthash 'a '(b c) triangle)
        (puthash 'b '(a c) triangle)
        (puthash 'c '(a b) triangle)
        ;; Bipartite (K2,3): 2
        (puthash 1 '(3 4 5) bipartite)
        (puthash 2 '(3 4 5) bipartite)
        (puthash 3 '(1 2) bipartite)
        (puthash 4 '(1 2) bipartite)
        (puthash 5 '(1 2) bipartite)
        ;; Odd cycle of 5: 3
        (puthash 'v1 '(v2 v5) petersen-like)
        (puthash 'v2 '(v1 v3) petersen-like)
        (puthash 'v3 '(v2 v4) petersen-like)
        (puthash 'v4 '(v3 v5) petersen-like)
        (puthash 'v5 '(v4 v1) petersen-like)
        (list
          (funcall 'neovm--gc-chromatic-number empty '(x))
          (funcall 'neovm--gc-chromatic-number edge '(a b))
          (funcall 'neovm--gc-chromatic-number triangle '(a b c))
          (funcall 'neovm--gc-chromatic-number bipartite '(1 2 3 4 5))
          (funcall 'neovm--gc-chromatic-number petersen-like '(v1 v2 v3 v4 v5))))
    (fmakunbound 'neovm--gc-is-safe)
    (fmakunbound 'neovm--gc-backtrack)
    (fmakunbound 'neovm--gc-chromatic-number)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Greedy coloring on different node orderings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_ordering_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Greedy coloring result depends on node ordering.
    // Show that the same graph can get different # of colors with different orderings.
    let form = r#"(progn
  (fset 'neovm--gc-greedy-color
    (lambda (adj-list nodes)
      (let ((colors (make-hash-table)))
        (dolist (node nodes)
          (let ((nbr-colors (make-hash-table)))
            (dolist (nbr (gethash node adj-list nil))
              (let ((c (gethash nbr colors)))
                (when c (puthash c t nbr-colors))))
            (let ((c 0))
              (while (gethash c nbr-colors) (setq c (1+ c)))
              (puthash node c colors))))
        colors)))

  (fset 'neovm--gc-count-colors
    (lambda (colors nodes)
      "Count distinct colors used."
      (let ((seen (make-hash-table)) (count 0))
        (dolist (n nodes)
          (let ((c (gethash n colors)))
            (unless (gethash c seen)
              (puthash c t seen)
              (setq count (1+ count)))))
        count)))

  (fset 'neovm--gc-verify
    (lambda (adj-list colors nodes)
      "Verify coloring is valid: no adjacent same-color nodes."
      (let ((valid t))
        (dolist (n nodes)
          (dolist (nbr (gethash n adj-list nil))
            (when (= (gethash n colors) (gethash nbr colors))
              (setq valid nil))))
        valid)))

  (unwind-protect
      (let ((g (make-hash-table)))
        ;; Crown graph (K3,3 minus perfect matching)
        ;; Nodes: a1 a2 a3 b1 b2 b3
        ;; Edges: ai-bj for i != j
        (puthash 'a1 '(b2 b3) g)
        (puthash 'a2 '(b1 b3) g)
        (puthash 'a3 '(b1 b2) g)
        (puthash 'b1 '(a2 a3) g)
        (puthash 'b2 '(a1 a3) g)
        (puthash 'b3 '(a1 a2) g)
        (let* ((order1 '(a1 a2 a3 b1 b2 b3))
               (order2 '(a1 b1 a2 b2 a3 b3))
               (order3 '(b3 b2 b1 a3 a2 a1))
               (c1 (funcall 'neovm--gc-greedy-color g order1))
               (c2 (funcall 'neovm--gc-greedy-color g order2))
               (c3 (funcall 'neovm--gc-greedy-color g order3))
               (all-nodes '(a1 a2 a3 b1 b2 b3)))
          (list
            ;; All colorings are valid
            (funcall 'neovm--gc-verify g c1 all-nodes)
            (funcall 'neovm--gc-verify g c2 all-nodes)
            (funcall 'neovm--gc-verify g c3 all-nodes)
            ;; Number of colors used by each ordering
            (funcall 'neovm--gc-count-colors c1 all-nodes)
            (funcall 'neovm--gc-count-colors c2 all-nodes)
            (funcall 'neovm--gc-count-colors c3 all-nodes)
            ;; Actual assignments for order1
            (let ((result nil))
              (dolist (n order1)
                (setq result (cons (cons n (gethash n c1)) result)))
              (nreverse result)))))
    (fmakunbound 'neovm--gc-greedy-color)
    (fmakunbound 'neovm--gc-count-colors)
    (fmakunbound 'neovm--gc-verify)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t 2 3 2 ((a1 . 0) (a2 . 0) (a3 . 0) (b1 . 1) (b2 . 1) (b3 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: register allocation as graph coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_register_allocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model register allocation: variables are nodes, interference (live
    // at the same time) creates edges, colors are registers.
    let form = r#"(progn
  (fset 'neovm--gc-greedy-color
    (lambda (adj-list nodes)
      (let ((colors (make-hash-table)))
        (dolist (node nodes)
          (let ((nbr-colors (make-hash-table)))
            (dolist (nbr (gethash node adj-list nil))
              (let ((c (gethash nbr colors)))
                (when c (puthash c t nbr-colors))))
            (let ((c 0))
              (while (gethash c nbr-colors) (setq c (1+ c)))
              (puthash node c colors))))
        colors)))

  (fset 'neovm--gc-build-interference
    (lambda (live-ranges)
      "Build interference graph from live ranges.
       LIVE-RANGES is a list of (variable start end).
       Two variables interfere if their ranges overlap."
      (let ((graph (make-hash-table))
            (vars (mapcar #'car live-ranges)))
        ;; Initialize all nodes
        (dolist (v vars) (puthash v nil graph))
        ;; Check each pair for overlap
        (let ((i 0))
          (while (< i (length live-ranges))
            (let* ((r1 (nth i live-ranges))
                   (v1 (nth 0 r1)) (s1 (nth 1 r1)) (e1 (nth 2 r1))
                   (j (1+ i)))
              (while (< j (length live-ranges))
                (let* ((r2 (nth j live-ranges))
                       (v2 (nth 0 r2)) (s2 (nth 1 r2)) (e2 (nth 2 r2)))
                  ;; Overlap: s1 < e2 and s2 < e1
                  (when (and (< s1 e2) (< s2 e1))
                    (puthash v1 (cons v2 (gethash v1 graph nil)) graph)
                    (puthash v2 (cons v1 (gethash v2 graph nil)) graph)))
                (setq j (1+ j))))
            (setq i (1+ i))))
        graph)))

  (unwind-protect
      (let* (;; Live ranges: (variable start-time end-time)
             (ranges '((a 0 5)   ;; a lives from 0 to 5
                       (b 1 3)   ;; b overlaps with a
                       (c 2 6)   ;; c overlaps with a and b
                       (d 4 8)   ;; d overlaps with a, c
                       (e 6 9)   ;; e overlaps with d
                       (f 7 10)  ;; f overlaps with d, e
                       (g 9 11)));; g overlaps with f
             (graph (funcall 'neovm--gc-build-interference ranges))
             (vars (mapcar #'car ranges))
             (reg-assignment (funcall 'neovm--gc-greedy-color graph vars))
             (reg-names '("R0" "R1" "R2" "R3" "R4" "R5")))
        ;; Verify and collect results
        (let ((valid t)
              (assignments nil)
              (max-reg 0))
          (dolist (v vars)
            (let ((reg (gethash v reg-assignment)))
              (setq max-reg (max max-reg reg))
              (setq assignments
                    (cons (list v (nth reg reg-names)) assignments))
              ;; Check no neighbor has same register
              (dolist (nbr (gethash v graph nil))
                (when (and (gethash nbr reg-assignment)
                           (= reg (gethash nbr reg-assignment)))
                  (setq valid nil)))))
          ;; Collect interference edges (sorted)
          (let ((edges nil))
            (dolist (v vars)
              (dolist (nbr (gethash v graph nil))
                (when (string< (symbol-name v) (symbol-name nbr))
                  (setq edges (cons (list v nbr) edges)))))
            (list
              'valid valid
              'registers-needed (1+ max-reg)
              'assignments (nreverse assignments)
              'interference-edges (sort edges
                (lambda (a b) (string< (symbol-name (car a))
                                       (symbol-name (car b)))))))))
    (fmakunbound 'neovm--gc-greedy-color)
    (fmakunbound 'neovm--gc-build-interference)))"#;
    let expect = expect_test::expect![[
        r#""OK (valid t registers-needed 3 assignments ((a \"R0\") (b \"R1\") (c \"R2\") (d \"R1\") (e \"R0\") (f \"R2\") (g \"R0\")) interference-edges ((a b) (a c) (a d) (b c) (c d) (d e) (d f) (e f) (f g)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: 4x4 Sudoku as graph coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_coloring_sudoku_4x4() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model a 4x4 Sudoku as a graph coloring problem.
    // Each cell is a node. Edges connect cells in same row, column, or 2x2 box.
    // Colors are digits 1-4. Pre-filled cells are constraints.
    let form = r#"(progn
  (fset 'neovm--gc-sudoku-neighbors
    (lambda (row col)
      "Return list of (r . c) pairs that conflict with (row . col) in 4x4 Sudoku."
      (let ((result nil)
            (box-r (* (/ row 2) 2))
            (box-c (* (/ col 2) 2)))
        ;; Same row
        (dotimes (c 4)
          (unless (= c col) (setq result (cons (cons row c) result))))
        ;; Same column
        (dotimes (r 4)
          (unless (= r row) (setq result (cons (cons r col) result))))
        ;; Same 2x2 box
        (dotimes (dr 2)
          (dotimes (dc 2)
            (let ((r (+ box-r dr)) (c (+ box-c dc)))
              (unless (and (= r row) (= c col))
                (unless (or (= r row) (= c col))  ;; avoid duplicates
                  (setq result (cons (cons r c) result)))))))
        result)))

  (fset 'neovm--gc-sudoku-solve
    (lambda (board)
      "Solve a 4x4 Sudoku. BOARD is a 4x4 vector of vectors (0=empty).
       Returns solved board or nil."
      ;; Find first empty cell
      (let ((empty-r nil) (empty-c nil))
        (let ((r 0))
          (while (and (< r 4) (not empty-r))
            (let ((c 0))
              (while (and (< c 4) (not empty-r))
                (when (= 0 (aref (aref board r) c))
                  (setq empty-r r)
                  (setq empty-c c))
                (setq c (1+ c))))
            (setq r (1+ r))))
        (if (not empty-r)
            board  ;; No empty cells = solved
          ;; Try each digit 1-4
          (let ((digit 1) (solution nil))
            (while (and (<= digit 4) (not solution))
              ;; Check if digit is safe
              (let ((safe t)
                    (nbrs (funcall 'neovm--gc-sudoku-neighbors empty-r empty-c)))
                (dolist (nbr nbrs)
                  (when (= digit (aref (aref board (car nbr)) (cdr nbr)))
                    (setq safe nil)))
                (when safe
                  ;; Place digit and recurse
                  (aset (aref board empty-r) empty-c digit)
                  (let ((result (funcall 'neovm--gc-sudoku-solve board)))
                    (if result
                        (setq solution result)
                      ;; Undo
                      (aset (aref board empty-r) empty-c 0)))))
              (setq digit (1+ digit)))
            solution)))))

  (fset 'neovm--gc-board-to-list
    (lambda (board)
      "Convert board to list of lists for deterministic output."
      (let ((result nil))
        (dotimes (r 4)
          (let ((row nil))
            (dotimes (c 4)
              (setq row (cons (aref (aref board r) c) row)))
            (setq result (cons (nreverse row) result))))
        (nreverse result))))

  (fset 'neovm--gc-verify-sudoku
    (lambda (board)
      "Verify a solved 4x4 Sudoku board."
      (let ((valid t))
        ;; Check rows
        (dotimes (r 4)
          (let ((seen (make-hash-table)))
            (dotimes (c 4)
              (let ((v (aref (aref board r) c)))
                (when (or (< v 1) (> v 4) (gethash v seen))
                  (setq valid nil))
                (puthash v t seen)))))
        ;; Check columns
        (dotimes (c 4)
          (let ((seen (make-hash-table)))
            (dotimes (r 4)
              (let ((v (aref (aref board r) c)))
                (when (gethash v seen) (setq valid nil))
                (puthash v t seen)))))
        ;; Check 2x2 boxes
        (dolist (box-start '((0 . 0) (0 . 2) (2 . 0) (2 . 2)))
          (let ((seen (make-hash-table)))
            (dotimes (dr 2)
              (dotimes (dc 2)
                (let ((v (aref (aref board (+ (car box-start) dr))
                               (+ (cdr box-start) dc))))
                  (when (gethash v seen) (setq valid nil))
                  (puthash v t seen))))))
        valid)))

  (unwind-protect
      (let* (;; Puzzle 1:
             ;; 1 _ | _ 4
             ;; _ _ | _ _
             ;; ----+----
             ;; _ _ | _ _
             ;; 3 _ | _ 2
             (board1 (vector (vector 1 0 0 4)
                             (vector 0 0 0 0)
                             (vector 0 0 0 0)
                             (vector 3 0 0 2)))
             (solved1 (funcall 'neovm--gc-sudoku-solve board1))
             ;; Puzzle 2: more constraints
             ;; _ 2 | 3 _
             ;; 3 _ | _ 2
             ;; ----+----
             ;; _ 3 | _ 1
             ;; 1 _ | 2 _
             (board2 (vector (vector 0 2 3 0)
                             (vector 3 0 0 2)
                             (vector 0 3 0 1)
                             (vector 1 0 2 0)))
             (solved2 (funcall 'neovm--gc-sudoku-solve board2)))
        (list
          ;; Puzzle 1 result
          (funcall 'neovm--gc-board-to-list solved1)
          (funcall 'neovm--gc-verify-sudoku solved1)
          ;; Puzzle 2 result
          (funcall 'neovm--gc-board-to-list solved2)
          (funcall 'neovm--gc-verify-sudoku solved2)
          ;; Neighbor count: each cell in 4x4 has exactly 7 neighbors
          ;; (3 in row + 3 in col + 1 in box not already counted)
          (length (funcall 'neovm--gc-sudoku-neighbors 0 0))
          (length (funcall 'neovm--gc-sudoku-neighbors 1 1))
          (length (funcall 'neovm--gc-sudoku-neighbors 3 3))))
    (fmakunbound 'neovm--gc-sudoku-neighbors)
    (fmakunbound 'neovm--gc-sudoku-solve)
    (fmakunbound 'neovm--gc-board-to-list)
    (fmakunbound 'neovm--gc-verify-sudoku)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
