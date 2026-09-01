//! Oracle parity tests for graph algorithm patterns implemented in
//! pure Elisp. Covers adjacency list construction, BFS with level
//! tracking, DFS with cycle detection, shortest path, connected
//! components, and bipartite graph checking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Adjacency list with weighted edges and neighbor queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_weighted_adjacency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a weighted directed graph using hash tables.
    // Each adjacency entry is (neighbor . weight).
    // Compute total weight of all edges, find heaviest edge,
    // and list all neighbors sorted by weight.
    let form = r#"(progn
      (fset 'neovm--test-add-weighted-edge
        (lambda (graph from to weight)
          (puthash from
                   (cons (cons to weight)
                         (gethash from graph nil))
                   graph)))
      (unwind-protect
          (let ((g (make-hash-table)))
            ;; Build graph
            (funcall 'neovm--test-add-weighted-edge g 'a 'b 4)
            (funcall 'neovm--test-add-weighted-edge g 'a 'c 2)
            (funcall 'neovm--test-add-weighted-edge g 'b 'c 5)
            (funcall 'neovm--test-add-weighted-edge g 'b 'd 10)
            (funcall 'neovm--test-add-weighted-edge g 'c 'd 3)
            (funcall 'neovm--test-add-weighted-edge g 'c 'e 8)
            (funcall 'neovm--test-add-weighted-edge g 'd 'e 1)
            (funcall 'neovm--test-add-weighted-edge g 'e 'a 7)
            ;; Total weight
            (let ((total 0)
                  (heaviest nil)
                  (max-weight -1))
              (let ((nodes '(a b c d e)))
                (dolist (n nodes)
                  (dolist (edge (gethash n g nil))
                    (let ((w (cdr edge)))
                      (setq total (+ total w))
                      (when (> w max-weight)
                        (setq max-weight w
                              heaviest (list n (car edge) w)))))))
              ;; Neighbors of 'c sorted by weight ascending
              (let ((c-neighbors (copy-sequence (gethash 'c g nil))))
                (setq c-neighbors
                      (sort c-neighbors (lambda (a b) (< (cdr a) (cdr b)))))
                (list total heaviest c-neighbors))))
        (fmakunbound 'neovm--test-add-weighted-edge)))"#;
    let expect = expect_test::expect![[r#""OK (40 (b d 10) ((d . 3) (e . 8)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BFS traversal with level tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_bfs_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS from a source, recording which level (distance) each node
    // is discovered at. Returns nodes grouped by level.
    let form = r#"(progn
      (fset 'neovm--test-bfs-levels
        (lambda (graph source)
          (let ((visited (make-hash-table))
                (level-map (make-hash-table))
                (queue (list (cons source 0)))
                (levels nil))
            (puthash source t visited)
            (puthash source 0 level-map)
            (while queue
              (let* ((item (car queue))
                     (node (car item))
                     (lvl (cdr item)))
                (setq queue (cdr queue))
                ;; Ensure level list exists
                (let ((existing (assq lvl levels)))
                  (if existing
                      (setcdr existing (cons node (cdr existing)))
                    (setq levels (cons (cons lvl (list node)) levels))))
                ;; Enqueue unvisited neighbors
                (dolist (neighbor (gethash node graph nil))
                  (unless (gethash neighbor visited)
                    (puthash neighbor t visited)
                    (puthash neighbor (1+ lvl) level-map)
                    (setq queue (append queue (list (cons neighbor (1+ lvl)))))))))
            ;; Sort levels by level number, sort nodes within each level
            (setq levels
                  (sort levels (lambda (a b) (< (car a) (car b)))))
            (mapcar (lambda (lvl-entry)
                      (cons (car lvl-entry)
                            (sort (cdr lvl-entry)
                                  (lambda (a b)
                                    (string< (symbol-name a) (symbol-name b))))))
                    levels))))
      (unwind-protect
          (let ((g (make-hash-table)))
            ;; Undirected graph (tree-like with some extra edges)
            (dolist (edge '((a b) (a c) (b d) (b e) (c f) (c g)
                           (d h) (e h) (f g)))
              (let ((u (car edge)) (v (cadr edge)))
                (puthash u (cons v (gethash u g nil)) g)
                (puthash v (cons u (gethash v g nil)) g)))
            (funcall 'neovm--test-bfs-levels g 'a))
        (fmakunbound 'neovm--test-bfs-levels)))"#;
    let expect = expect_test::expect![[r#""OK ((0 a) (1 b c) (2 d e f g) (3 h))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFS with cycle detection and back-edge reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_dfs_cycle_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Iterative DFS on directed graph with tri-color marking.
    // Reports all back edges found (not just presence of cycle).
    let form = r#"(progn
      (fset 'neovm--test-dfs-find-back-edges
        (lambda (graph nodes)
          (let ((color (make-hash-table))
                (back-edges nil)
                (finish-order nil))
            ;; Initialize all white
            (dolist (n nodes) (puthash n 'white color))
            ;; Recursive DFS visit
            (fset 'neovm--test-dfs-visit-be
              (lambda (u)
                (puthash u 'gray color)
                (dolist (v (gethash u graph nil))
                  (let ((c (gethash v color 'white)))
                    (cond
                      ((eq c 'white)
                       (funcall 'neovm--test-dfs-visit-be v))
                      ((eq c 'gray)
                       (setq back-edges (cons (cons u v) back-edges))))))
                (puthash u 'black color)
                (setq finish-order (cons u finish-order))))
            (dolist (n nodes)
              (when (eq (gethash n color) 'white)
                (funcall 'neovm--test-dfs-visit-be n)))
            (list (nreverse back-edges) finish-order))))
      (unwind-protect
          (let ((g1 (make-hash-table))
                (g2 (make-hash-table)))
            ;; g1: DAG
            (puthash 'a '(b c) g1)
            (puthash 'b '(d) g1)
            (puthash 'c '(d e) g1)
            (puthash 'd '(f) g1)
            (puthash 'e '(f) g1)
            (puthash 'f nil g1)
            ;; g2: multiple cycles a->b->c->a and d->e->d
            (puthash 'a '(b) g2)
            (puthash 'b '(c d) g2)
            (puthash 'c '(a) g2)
            (puthash 'd '(e) g2)
            (puthash 'e '(d f) g2)
            (puthash 'f nil g2)
            (let ((r1 (funcall 'neovm--test-dfs-find-back-edges
                               g1 '(a b c d e f)))
                  (r2 (funcall 'neovm--test-dfs-find-back-edges
                               g2 '(a b c d e f))))
              (list (list 'dag-back-edges (car r1))
                    (list 'dag-finish (cadr r1))
                    (list 'cyclic-back-edges (car r2))
                    (list 'cyclic-finish (cadr r2)))))
        (fmakunbound 'neovm--test-dfs-find-back-edges)
        (fmakunbound 'neovm--test-dfs-visit-be)))"#;
    let expect = expect_test::expect![[
        r#""OK ((dag-back-edges nil) (dag-finish (a c e b d f)) (cyclic-back-edges ((c . a) (e . d))) (cyclic-finish (a b d e f c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shortest path (BFS) with path reconstruction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_shortest_path_reconstruct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS shortest path from every node to every other node (all-pairs).
    // Returns a distance matrix and the actual paths for selected pairs.
    let form = r#"(progn
      (fset 'neovm--test-bfs-shortest
        (lambda (graph source)
          (let ((dist (make-hash-table))
                (parent (make-hash-table))
                (queue (list source)))
            (puthash source 0 dist)
            (while queue
              (let ((u (car queue)))
                (setq queue (cdr queue))
                (dolist (v (gethash u graph nil))
                  (unless (gethash v dist)
                    (puthash v (1+ (gethash u dist)) dist)
                    (puthash v u parent)
                    (setq queue (append queue (list v)))))))
            (cons dist parent))))
      (fset 'neovm--test-reconstruct-path
        (lambda (parent-map target)
          (let ((path nil)
                (cur target))
            (while cur
              (setq path (cons cur path))
              (setq cur (gethash cur parent-map)))
            path)))
      (unwind-protect
          (let ((g (make-hash-table))
                (nodes '(1 2 3 4 5 6)))
            ;; Undirected graph
            (dolist (edge '((1 2) (1 3) (2 3) (2 4) (3 5) (4 5) (4 6) (5 6)))
              (let ((u (car edge)) (v (cadr edge)))
                (puthash u (cons v (gethash u g nil)) g)
                (puthash v (cons u (gethash v g nil)) g)))
            ;; BFS from node 1
            (let* ((result (funcall 'neovm--test-bfs-shortest g 1))
                   (dist-map (car result))
                   (par-map (cdr result)))
              ;; Collect distances
              (let ((distances nil))
                (dolist (n nodes)
                  (setq distances
                        (cons (cons n (gethash n dist-map -1))
                              distances)))
                ;; Reconstruct paths from 1 to 6 and from 1 to 5
                (let ((path-1-6 (funcall 'neovm--test-reconstruct-path par-map 6))
                      (path-1-5 (funcall 'neovm--test-reconstruct-path par-map 5)))
                  (list (sort (nreverse distances)
                              (lambda (a b) (< (car a) (car b))))
                        path-1-6
                        (length path-1-6)
                        path-1-5
                        (length path-1-5))))))
        (fmakunbound 'neovm--test-bfs-shortest)
        (fmakunbound 'neovm--test-reconstruct-path)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 0) (2 . 1) (3 . 1) (4 . 2) (5 . 2) (6 . 3)) (1 3 5 6) 4 (1 3 5) 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connected components with component sizes and statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_connected_components_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find connected components, compute statistics: number of components,
    // sizes, largest component, isolated nodes.
    let form = r#"(progn
      (fset 'neovm--test-find-components
        (lambda (graph all-nodes)
          (let ((visited (make-hash-table))
                (components nil))
            (dolist (start all-nodes)
              (unless (gethash start visited)
                (let ((queue (list start))
                      (component nil))
                  (puthash start t visited)
                  (while queue
                    (let ((node (car queue)))
                      (setq queue (cdr queue))
                      (setq component (cons node component))
                      (dolist (nbr (gethash node graph nil))
                        (unless (gethash nbr visited)
                          (puthash nbr t visited)
                          (setq queue (append queue (list nbr)))))))
                  (setq components
                        (cons (sort component
                                    (lambda (a b) (< a b)))
                              components)))))
            (sort (nreverse components)
                  (lambda (a b) (< (car a) (car b)))))))
      (unwind-protect
          (let ((g (make-hash-table))
                (nodes '(1 2 3 4 5 6 7 8 9 10 11 12)))
            ;; Component 1: {1,2,3,4,5} - a pentagon
            (dolist (edge '((1 2) (2 3) (3 4) (4 5) (5 1)))
              (let ((u (car edge)) (v (cadr edge)))
                (puthash u (cons v (gethash u g nil)) g)
                (puthash v (cons u (gethash v g nil)) g)))
            ;; Component 2: {6,7} - single edge
            (puthash 6 (cons 7 (gethash 6 g nil)) g)
            (puthash 7 (cons 6 (gethash 7 g nil)) g)
            ;; Component 3: {8,9,10} - triangle
            (dolist (edge '((8 9) (9 10) (10 8)))
              (let ((u (car edge)) (v (cadr edge)))
                (puthash u (cons v (gethash u g nil)) g)
                (puthash v (cons u (gethash v g nil)) g)))
            ;; Isolated nodes: 11, 12
            (dolist (n nodes)
              (unless (gethash n g)
                (puthash n nil g)))
            (let ((comps (funcall 'neovm--test-find-components g nodes)))
              (let ((sizes (mapcar #'length comps))
                    (largest (car (sort (copy-sequence comps)
                                        (lambda (a b) (> (length a) (length b))))))
                    (isolated (let ((iso nil))
                                (dolist (c comps)
                                  (when (= (length c) 1)
                                    (setq iso (append iso c))))
                                iso)))
                (list (length comps) sizes largest isolated comps))))
        (fmakunbound 'neovm--test-find-components)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (5 2 3 1 1) (1 2 3 4 5) (11 12) ((1 2 3 4 5) (6 7) (8 9 10) (11) (12)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bipartite graph checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_bipartite_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Check if an undirected graph is bipartite using BFS 2-coloring.
    // If bipartite, return the two partitions. If not, return the
    // odd-cycle evidence (the conflicting edge).
    let form = r#"(progn
      (fset 'neovm--test-is-bipartite
        (lambda (graph all-nodes)
          (let ((color-map (make-hash-table))
                (is-bipartite t)
                (conflict nil))
            ;; BFS coloring for each unvisited component
            (dolist (start all-nodes)
              (unless (or (not is-bipartite) (gethash start color-map))
                (puthash start 0 color-map)
                (let ((queue (list start)))
                  (while (and queue is-bipartite)
                    (let ((u (car queue)))
                      (setq queue (cdr queue))
                      (let ((u-color (gethash u color-map)))
                        (dolist (v (gethash u graph nil))
                          (let ((v-color (gethash v color-map)))
                            (cond
                              ((null v-color)
                               (puthash v (- 1 u-color) color-map)
                               (setq queue (append queue (list v))))
                              ((= v-color u-color)
                               (setq is-bipartite nil
                                     conflict (list u v u-color))))))))))))
            (if is-bipartite
                ;; Build partitions
                (let ((part-0 nil) (part-1 nil))
                  (dolist (n all-nodes)
                    (if (= (gethash n color-map 0) 0)
                        (setq part-0 (cons n part-0))
                      (setq part-1 (cons n part-1))))
                  (list t
                        (sort (nreverse part-0) #'<)
                        (sort (nreverse part-1) #'<)))
              (list nil conflict)))))
      (unwind-protect
          (let ((results nil))
            ;; Bipartite graph (a simple path 1-2-3-4)
            (let ((g1 (make-hash-table)))
              (dolist (edge '((1 2) (2 3) (3 4)))
                (let ((u (car edge)) (v (cadr edge)))
                  (puthash u (cons v (gethash u g1 nil)) g1)
                  (puthash v (cons u (gethash v g1 nil)) g1)))
              (setq results
                    (cons (list 'path-graph
                                (funcall 'neovm--test-is-bipartite g1 '(1 2 3 4)))
                          results)))
            ;; Bipartite graph (complete bipartite K_{2,3})
            (let ((g2 (make-hash-table)))
              (dolist (edge '((1 3) (1 4) (1 5) (2 3) (2 4) (2 5)))
                (let ((u (car edge)) (v (cadr edge)))
                  (puthash u (cons v (gethash u g2 nil)) g2)
                  (puthash v (cons u (gethash v g2 nil)) g2)))
              (setq results
                    (cons (list 'k23
                                (funcall 'neovm--test-is-bipartite g2 '(1 2 3 4 5)))
                          results)))
            ;; Non-bipartite: triangle (odd cycle)
            (let ((g3 (make-hash-table)))
              (dolist (edge '((1 2) (2 3) (3 1)))
                (let ((u (car edge)) (v (cadr edge)))
                  (puthash u (cons v (gethash u g3 nil)) g3)
                  (puthash v (cons u (gethash v g3 nil)) g3)))
              (setq results
                    (cons (list 'triangle
                                (car (funcall 'neovm--test-is-bipartite g3 '(1 2 3))))
                          results)))
            ;; Non-bipartite: pentagon (5-cycle, odd)
            (let ((g4 (make-hash-table)))
              (dolist (edge '((1 2) (2 3) (3 4) (4 5) (5 1)))
                (let ((u (car edge)) (v (cadr edge)))
                  (puthash u (cons v (gethash u g4 nil)) g4)
                  (puthash v (cons u (gethash v g4 nil)) g4)))
              (setq results
                    (cons (list 'pentagon
                                (car (funcall 'neovm--test-is-bipartite g4 '(1 2 3 4 5))))
                          results)))
            (nreverse results))
        (fmakunbound 'neovm--test-is-bipartite)))"#;
    let expect = expect_test::expect![[
        r#""OK ((path-graph (t (1 3) (2 4))) (k23 (t (1 2) (3 4 5))) (triangle nil) (pentagon nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_pattern_topological_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kahn's algorithm for topological sort on a DAG.
    // Computes in-degrees, repeatedly removes zero-indegree nodes.
    // Verifies correctness: every edge (u,v), u appears before v in result.
    let form = r#"(progn
      (fset 'neovm--test-topo-sort
        (lambda (graph nodes)
          (let ((in-degree (make-hash-table))
                (result nil)
                (queue nil))
            ;; Initialize in-degrees to 0
            (dolist (n nodes) (puthash n 0 in-degree))
            ;; Count incoming edges
            (dolist (u nodes)
              (dolist (v (gethash u graph nil))
                (puthash v (1+ (gethash v in-degree 0)) in-degree)))
            ;; Enqueue nodes with in-degree 0
            (dolist (n nodes)
              (when (= (gethash n in-degree) 0)
                (setq queue (append queue (list n)))))
            ;; Process queue
            (while queue
              (let ((u (car queue)))
                (setq queue (cdr queue))
                (setq result (cons u result))
                (dolist (v (gethash u graph nil))
                  (puthash v (1- (gethash v in-degree)) in-degree)
                  (when (= (gethash v in-degree) 0)
                    (setq queue (append queue (list v)))))))
            ;; If result length != nodes length, graph has a cycle
            (list (= (length result) (length nodes))
                  (nreverse result)))))
      (unwind-protect
          (let ((g (make-hash-table)))
            ;; DAG representing course prerequisites
            (puthash 'math101 '(math201 cs101) g)
            (puthash 'cs101 '(cs201 cs202) g)
            (puthash 'math201 '(cs301) g)
            (puthash 'cs201 '(cs301) g)
            (puthash 'cs202 '(cs301) g)
            (puthash 'cs301 nil g)
            (let* ((nodes '(math101 cs101 math201 cs201 cs202 cs301))
                   (result (funcall 'neovm--test-topo-sort g nodes))
                   (valid (car result))
                   (order (cadr result)))
              ;; Verify: for each edge (u,v), position of u < position of v
              (let ((pos-map (make-hash-table))
                    (all-ok t))
                (let ((idx 0))
                  (dolist (n order)
                    (puthash n idx pos-map)
                    (setq idx (1+ idx))))
                (dolist (u nodes)
                  (dolist (v (gethash u g nil))
                    (when (>= (gethash u pos-map) (gethash v pos-map))
                      (setq all-ok nil))))
                (list valid order all-ok))))
        (fmakunbound 'neovm--test-topo-sort)))"#;
    let expect = expect_test::expect![[r#""OK (t (math101 math201 cs101 cs201 cs202 cs301) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
