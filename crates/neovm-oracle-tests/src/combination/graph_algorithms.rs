//! Complex oracle parity tests for graph algorithms implemented in
//! Elisp using hash tables for adjacency lists, visited sets, and
//! queue/stack-based traversals.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Adjacency list representation with hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adjacency_list_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a directed graph from an edge list, query neighbors,
    // compute in-degree and out-degree for each node.
    let form = r#"(let ((graph (make-hash-table))
                        (edges '((a b) (a c) (b d) (c d) (c e) (d e) (e a))))
      ;; Build adjacency list
      (dolist (edge edges)
        (let ((from (car edge))
              (to (cadr edge)))
          (puthash from (cons to (gethash from graph nil)) graph)))
      ;; Compute out-degree for each node
      (let ((nodes '(a b c d e))
            (out-degrees nil)
            (in-degrees nil))
        (dolist (n nodes)
          (setq out-degrees
                (cons (cons n (length (gethash n graph nil)))
                      out-degrees)))
        ;; Compute in-degree by scanning all adjacency lists
        (dolist (n nodes)
          (let ((count 0))
            (dolist (src nodes)
              (when (memq n (gethash src graph nil))
                (setq count (1+ count))))
            (setq in-degrees (cons (cons n count) in-degrees))))
        (list (sort (nreverse out-degrees)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              (sort (nreverse in-degrees)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 2) (b . 1) (c . 2) (d . 1) (e . 1)) ((a . 1) (b . 1) (c . 1) (d . 2) (e . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BFS with visited set and path tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_bfs_path_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS that finds shortest path (by number of edges) between two
    // nodes in an undirected graph. Returns both the visit order and
    // the path.
    let form = r#"(let ((graph (make-hash-table)))
      ;; Undirected graph: add edges both ways
      (let ((edges '((a b) (a c) (b d) (b e) (c f) (d g) (e g) (f g))))
        (dolist (edge edges)
          (let ((u (car edge)) (v (cadr edge)))
            (puthash u (cons v (gethash u graph nil)) graph)
            (puthash v (cons u (gethash v graph nil)) graph))))
      ;; BFS from 'a to 'g, tracking parent pointers
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list 'a))
            (visit-order nil)
            (start 'a)
            (goal 'g))
        (puthash start t visited)
        (puthash start nil parent)
        (let ((found nil))
          (while (and queue (not found))
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq visit-order (cons node visit-order))
              (when (eq node goal)
                (setq found t))
              (unless found
                (dolist (neighbor (gethash node graph))
                  (unless (gethash neighbor visited)
                    (puthash neighbor t visited)
                    (puthash neighbor node parent)
                    (setq queue (append queue (list neighbor))))))))
          ;; Reconstruct path from goal to start
          (let ((path nil)
                (current goal))
            (while current
              (setq path (cons current path))
              (setq current (gethash current parent)))
            (list (nreverse visit-order)
                  path
                  (length path)
                  found)))))"#;
    let expect = expect_test::expect![[r#""OK ((a c b f e d g) (a c f g) 4 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFS with cycle detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_dfs_cycle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFS on a directed graph that detects back edges (cycles).
    // Uses iterative DFS with explicit stack and coloring
    // (white=unvisited, gray=in-progress, black=done).
    let form = r#"(progn
      (fset 'neovm--test-dfs-cycle
        (lambda (graph nodes)
          (let ((color (make-hash-table))
                (has-cycle nil)
                (topo-order nil))
            ;; Initialize all as white (0)
            (dolist (n nodes) (puthash n 0 color))
            ;; DFS visit function (recursive via fset)
            (fset 'neovm--test-dfs-visit
              (lambda (node)
                (puthash node 1 color)  ;; gray
                (dolist (neighbor (gethash node graph nil))
                  (cond
                    ((= (gethash neighbor color 0) 1)
                     ;; Gray neighbor = back edge = cycle
                     (setq has-cycle t))
                    ((= (gethash neighbor color 0) 0)
                     (funcall 'neovm--test-dfs-visit neighbor))))
                (puthash node 2 color)  ;; black
                (setq topo-order (cons node topo-order))))
            ;; Visit all nodes
            (dolist (n nodes)
              (when (= (gethash n color) 0)
                (funcall 'neovm--test-dfs-visit n)))
            (list has-cycle topo-order))))
      (unwind-protect
          (let ((g1 (make-hash-table))
                (g2 (make-hash-table)))
            ;; g1: DAG (no cycle)
            (puthash 'a '(b c) g1)
            (puthash 'b '(d) g1)
            (puthash 'c '(d) g1)
            (puthash 'd nil g1)
            ;; g2: has cycle a->b->c->a
            (puthash 'a '(b) g2)
            (puthash 'b '(c) g2)
            (puthash 'c '(a d) g2)
            (puthash 'd nil g2)
            (let ((r1 (funcall 'neovm--test-dfs-cycle g1 '(a b c d)))
                  (r2 (funcall 'neovm--test-dfs-cycle g2 '(a b c d))))
              (list (car r1)   ;; nil (no cycle)
                    (car r2)   ;; t (has cycle)
                    )))
        (fmakunbound 'neovm--test-dfs-cycle)
        (fmakunbound 'neovm--test-dfs-visit)))"#;
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shortest path (unweighted) using BFS
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_shortest_path_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute shortest distances from a source node to ALL other nodes
    // in an undirected graph using BFS. Returns distance map.
    let form = r#"(let ((graph (make-hash-table)))
      ;; Build undirected graph
      (dolist (edge '((1 2) (1 3) (2 4) (3 4) (3 5) (4 6) (5 6) (5 7) (6 7)))
        (let ((u (car edge)) (v (cadr edge)))
          (puthash u (cons v (gethash u graph nil)) graph)
          (puthash v (cons u (gethash v graph nil)) graph)))
      ;; BFS from node 1
      (let ((dist (make-hash-table))
            (queue (list 1)))
        (puthash 1 0 dist)
        (while queue
          (let ((node (car queue)))
            (setq queue (cdr queue))
            (dolist (neighbor (gethash node graph))
              (unless (gethash neighbor dist)
                (puthash neighbor (1+ (gethash node dist)) dist)
                (setq queue (append queue (list neighbor)))))))
        ;; Collect sorted (node . distance) pairs
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k v) result))) dist)
          (sort result (lambda (a b) (< (car a) (car b)))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 . 0) (2 . 1) (3 . 1) (4 . 2) (5 . 2) (6 . 3) (7 . 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connected components detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_connected_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find connected components in an undirected graph using BFS.
    // Returns the number of components and the nodes in each.
    let form = r#"(let ((graph (make-hash-table))
                        (all-nodes '(1 2 3 4 5 6 7 8 9)))
      ;; Three components: {1,2,3,4}, {5,6}, {7,8,9}
      (dolist (edge '((1 2) (2 3) (3 4) (1 4)
                      (5 6)
                      (7 8) (8 9) (7 9)))
        (let ((u (car edge)) (v (cadr edge)))
          (puthash u (cons v (gethash u graph nil)) graph)
          (puthash v (cons u (gethash v graph nil)) graph)))
      ;; Ensure isolated nodes have entries
      (dolist (n all-nodes)
        (unless (gethash n graph)
          (puthash n nil graph)))
      ;; Find components via repeated BFS
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
                  (dolist (neighbor (gethash node graph nil))
                    (unless (gethash neighbor visited)
                      (puthash neighbor t visited)
                      (setq queue (append queue (list neighbor)))))))
              (setq components
                    (cons (sort component #'<) components)))))
        (let ((sorted-components
               (sort (nreverse components)
                     (lambda (a b) (< (car a) (car b))))))
          (list (length sorted-components)
                sorted-components))))"#;
    let expect = expect_test::expect![[r#""OK (3 ((1 2 3 4) (5 6) (7 8 9)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Graph transpose (reverse all edges)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a directed graph, compute its transpose (reverse all edges),
    // then verify properties: if (u,v) is in G, then (v,u) is in G^T,
    // and out-degree of v in G equals in-degree of v in G^T (which is
    // out-degree of v in G^T when viewed from v's adjacency list).
    let form = r#"(let ((graph (make-hash-table))
                        (nodes '(a b c d e)))
      ;; Build directed graph
      (puthash 'a '(b c) graph)
      (puthash 'b '(d) graph)
      (puthash 'c '(d e) graph)
      (puthash 'd '(e) graph)
      (puthash 'e '(a) graph)
      ;; Compute transpose
      (let ((transposed (make-hash-table)))
        ;; Initialize all nodes
        (dolist (n nodes) (puthash n nil transposed))
        ;; Reverse each edge
        (dolist (src nodes)
          (dolist (dst (gethash src graph nil))
            (puthash dst (cons src (gethash dst transposed nil)) transposed)))
        ;; Sort adjacency lists for deterministic output
        (let ((orig-adj nil)
              (trans-adj nil))
          (dolist (n nodes)
            (setq orig-adj
                  (cons (cons n (sort (copy-sequence (gethash n graph nil))
                                      (lambda (a b) (string< (symbol-name a)
                                                             (symbol-name b)))))
                        orig-adj))
            (setq trans-adj
                  (cons (cons n (sort (copy-sequence (gethash n transposed nil))
                                      (lambda (a b) (string< (symbol-name a)
                                                             (symbol-name b)))))
                        trans-adj)))
          ;; Verify: total edge count should be the same
          (let ((orig-edge-count 0)
                (trans-edge-count 0))
            (dolist (n nodes)
              (setq orig-edge-count (+ orig-edge-count
                                       (length (gethash n graph nil))))
              (setq trans-edge-count (+ trans-edge-count
                                        (length (gethash n transposed nil)))))
            (list
              (sort (nreverse orig-adj)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              (sort (nreverse trans-adj)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              (= orig-edge-count trans-edge-count)
              orig-edge-count)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b c) (b d) (c d e) (d e) (e a)) ((a e) (b a) (c a) (d b c) (e c d)) t 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
