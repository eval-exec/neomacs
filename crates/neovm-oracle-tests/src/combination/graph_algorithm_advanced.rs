//! Advanced oracle parity tests for graph algorithms in Elisp:
//! Dijkstra's shortest path with priority queue, topological sort (Kahn's),
//! strongly connected components (Tarjan's), bipartite checking,
//! Euler path detection, minimum spanning tree (Prim's),
//! and graph cycle detection with path reporting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Dijkstra's shortest path with a priority queue (list-based min-heap)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_dijkstra_shortest_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Weighted directed graph. Dijkstra finds shortest distance from source
    // to all reachable nodes. Priority queue simulated with sorted insertion.
    let form = r#"(progn
  ;; Graph as hash-table: node -> list of (neighbor . weight)
  (fset 'neovm--ga-dijkstra
    (lambda (graph source)
      (let ((dist (make-hash-table :test 'eq))
            (visited (make-hash-table :test 'eq))
            ;; pq: sorted list of (distance . node)
            (pq (list (cons 0 source))))
        (puthash source 0 dist)
        (while pq
          (let* ((top (car pq))
                 (d (car top))
                 (u (cdr top)))
            (setq pq (cdr pq))
            (unless (gethash u visited)
              (puthash u t visited)
              ;; Relax neighbors
              (dolist (edge (gethash u graph nil))
                (let* ((v (car edge))
                       (w (cdr edge))
                       (new-dist (+ d w))
                       (old-dist (gethash v dist most-positive-fixnum)))
                  (when (< new-dist old-dist)
                    (puthash v new-dist dist)
                    ;; Insert into pq maintaining sorted order
                    (let ((entry (cons new-dist v))
                          (new-pq nil)
                          (inserted nil))
                      (dolist (item pq)
                        (when (and (not inserted) (< new-dist (car item)))
                          (setq new-pq (cons entry new-pq))
                          (setq inserted t))
                        (setq new-pq (cons item new-pq)))
                      (unless inserted
                        (setq new-pq (cons entry new-pq)))
                      (setq pq (nreverse new-pq)))))))))
        dist)))

  (unwind-protect
      (let ((g (make-hash-table :test 'eq)))
        ;; Build weighted graph:
        ;; A->B:1, A->C:4, B->C:2, B->D:5, C->D:1, C->E:3, D->E:1
        (puthash 'A '((B . 1) (C . 4)) g)
        (puthash 'B '((C . 2) (D . 5)) g)
        (puthash 'C '((D . 1) (E . 3)) g)
        (puthash 'D '((E . 1)) g)
        (let ((result (funcall 'neovm--ga-dijkstra g 'A)))
          ;; Collect distances sorted by node name
          (let ((pairs nil))
            (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) result)
            (sort pairs (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b))))))))
    (fmakunbound 'neovm--ga-dijkstra)))"#;
    let expect = expect_test::expect![[r#""OK ((A . 0) (B . 1) (C . 3) (D . 4) (E . 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_topological_sort_kahn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kahn's algorithm: repeatedly remove nodes with in-degree 0.
    // Returns a valid topological ordering or nil if cycle exists.
    let form = r#"(progn
  (fset 'neovm--ga-topo-sort
    (lambda (nodes edges)
      "Topological sort using Kahn's algorithm.
NODES: list of node symbols. EDGES: list of (from . to) pairs."
      (let ((in-degree (make-hash-table :test 'eq))
            (adj (make-hash-table :test 'eq))
            (queue nil)
            (result nil))
        ;; Initialize
        (dolist (n nodes) (puthash n 0 in-degree))
        (dolist (e edges)
          (let ((from (car e)) (to (cdr e)))
            (puthash from (cons to (gethash from adj nil)) adj)
            (puthash to (1+ (gethash to in-degree 0)) in-degree)))
        ;; Seed queue with in-degree 0 nodes (sorted for determinism)
        (dolist (n nodes)
          (when (= 0 (gethash n in-degree 0))
            (setq queue (cons n queue))))
        (setq queue (sort queue (lambda (a b)
                                  (string< (symbol-name a) (symbol-name b)))))
        ;; Process
        (while queue
          (let ((u (car queue)))
            (setq queue (cdr queue))
            (setq result (cons u result))
            (dolist (v (gethash u adj nil))
              (puthash v (1- (gethash v in-degree)) in-degree)
              (when (= 0 (gethash v in-degree))
                ;; Insert into sorted queue for determinism
                (let ((new-q nil) (inserted nil))
                  (dolist (item queue)
                    (when (and (not inserted)
                               (string< (symbol-name v) (symbol-name item)))
                      (setq new-q (cons v new-q))
                      (setq inserted t))
                    (setq new-q (cons item new-q)))
                  (unless inserted (setq new-q (cons v new-q)))
                  (setq queue (nreverse new-q)))))))
        ;; Check if all nodes processed (no cycle)
        (if (= (length result) (length nodes))
            (nreverse result)
          nil))))

  (unwind-protect
      (list
       ;; DAG: A->B, A->C, B->D, C->D, D->E
       (funcall 'neovm--ga-topo-sort
                '(A B C D E)
                '((A . B) (A . C) (B . D) (C . D) (D . E)))
       ;; More complex DAG
       (funcall 'neovm--ga-topo-sort
                '(A B C D E F)
                '((A . B) (A . C) (B . D) (C . D) (C . E) (D . F) (E . F)))
       ;; Single node, no edges
       (funcall 'neovm--ga-topo-sort '(X) nil)
       ;; Two disconnected components
       (funcall 'neovm--ga-topo-sort
                '(A B C D)
                '((A . B) (C . D))))
    (fmakunbound 'neovm--ga-topo-sort)))"#;
    let expect = expect_test::expect![[r#""OK ((A B C D E) (A B C D E F) (X) (A B C D))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Strongly connected components (Tarjan's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_tarjan_scc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tarjan's SCC algorithm using iterative DFS with explicit stack.
    let form = r#"(progn
  (fset 'neovm--ga-tarjan-scc
    (lambda (nodes adj)
      "Find SCCs. ADJ is hash-table node->list of neighbors."
      (let ((index-counter (list 0))  ;; mutable counter via list
            (stack nil)
            (on-stack (make-hash-table :test 'eq))
            (node-index (make-hash-table :test 'eq))
            (node-lowlink (make-hash-table :test 'eq))
            (sccs nil))
        (fset 'neovm--ga-tarjan-visit
          (lambda (v)
            (let ((idx (car index-counter)))
              (puthash v idx node-index)
              (puthash v idx node-lowlink)
              (setcar index-counter (1+ idx))
              (setq stack (cons v stack))
              (puthash v t on-stack)
              ;; Visit neighbors
              (dolist (w (gethash v adj nil))
                (cond
                 ((null (gethash w node-index nil))
                  ;; Not visited
                  (funcall 'neovm--ga-tarjan-visit w)
                  (puthash v (min (gethash v node-lowlink)
                                  (gethash w node-lowlink))
                           node-lowlink))
                 ((gethash w on-stack)
                  ;; On stack -> back edge
                  (puthash v (min (gethash v node-lowlink)
                                  (gethash w node-index))
                           node-lowlink))))
              ;; Root of SCC?
              (when (= (gethash v node-lowlink) (gethash v node-index))
                (let ((scc nil) (done nil))
                  (while (not done)
                    (let ((w (car stack)))
                      (setq stack (cdr stack))
                      (puthash w nil on-stack)
                      (setq scc (cons w scc))
                      (when (eq w v) (setq done t))))
                  (setq sccs (cons (sort scc
                                         (lambda (a b)
                                           (string< (symbol-name a)
                                                    (symbol-name b))))
                                   sccs)))))))
        ;; Visit all nodes
        (dolist (n nodes)
          (unless (gethash n node-index nil)
            (funcall 'neovm--ga-tarjan-visit n)))
        ;; Sort SCCs for deterministic output
        (sort sccs (lambda (a b)
                     (string< (symbol-name (car a))
                              (symbol-name (car b))))))))

  (unwind-protect
      (let ((adj (make-hash-table :test 'eq)))
        ;; Graph with 3 SCCs: {A,B,C}, {D}, {E,F}
        (puthash 'A '(B) adj)
        (puthash 'B '(C) adj)
        (puthash 'C '(A D) adj)
        (puthash 'D '(E) adj)
        (puthash 'E '(F) adj)
        (puthash 'F '(E) adj)
        (funcall 'neovm--ga-tarjan-scc
                 '(A B C D E F) adj))
    (fmakunbound 'neovm--ga-tarjan-scc)
    (fmakunbound 'neovm--ga-tarjan-visit)))"#;
    let expect = expect_test::expect![[r#""OK ((A B C) (D) (E F))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bipartite graph checking via BFS coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_bipartite_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Check if an undirected graph is bipartite using two-coloring BFS.
    let form = r#"(progn
  (fset 'neovm--ga-is-bipartite
    (lambda (nodes adj)
      "Check if graph is bipartite. ADJ: hash-table node->neighbors (undirected)."
      (let ((color (make-hash-table :test 'eq))
            (result t))
        (dolist (start nodes)
          (unless (gethash start color)
            ;; BFS from unvisited node
            (puthash start 0 color)
            (let ((queue (list start)))
              (while (and queue result)
                (let ((u (car queue)))
                  (setq queue (cdr queue))
                  (let ((u-color (gethash u color)))
                    (dolist (v (gethash u adj nil))
                      (let ((v-color (gethash v color)))
                        (cond
                         ((null v-color)
                          (puthash v (- 1 u-color) color)
                          (setq queue (append queue (list v))))
                         ((= v-color u-color)
                          (setq result nil)))))))))))
        result)))

  (unwind-protect
      (let ((adj1 (make-hash-table :test 'eq))
            (adj2 (make-hash-table :test 'eq))
            (adj3 (make-hash-table :test 'eq)))
        ;; Bipartite graph (even cycle): A-B-C-D-A
        (puthash 'A '(B D) adj1)
        (puthash 'B '(A C) adj1)
        (puthash 'C '(B D) adj1)
        (puthash 'D '(C A) adj1)
        ;; Non-bipartite (odd cycle): A-B-C-A
        (puthash 'A '(B C) adj2)
        (puthash 'B '(A C) adj2)
        (puthash 'C '(B A) adj2)
        ;; Bipartite tree: star graph
        (puthash 'center '(L1 L2 L3 L4) adj3)
        (puthash 'L1 '(center) adj3)
        (puthash 'L2 '(center) adj3)
        (puthash 'L3 '(center) adj3)
        (puthash 'L4 '(center) adj3)
        (list
         (funcall 'neovm--ga-is-bipartite '(A B C D) adj1)
         (funcall 'neovm--ga-is-bipartite '(A B C) adj2)
         (funcall 'neovm--ga-is-bipartite '(center L1 L2 L3 L4) adj3)))
    (fmakunbound 'neovm--ga-is-bipartite)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Euler path detection (check degree parity)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_euler_path_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An undirected connected graph has an Euler path iff it has 0 or 2
    // vertices of odd degree. An Euler circuit iff all vertices have even degree.
    let form = r#"(progn
  (fset 'neovm--ga-euler-check
    (lambda (nodes adj)
      "Check Euler path/circuit possibility.
Returns 'circuit if Euler circuit, 'path if Euler path, 'neither otherwise."
      (let ((odd-count 0)
            (degrees nil))
        (dolist (n nodes)
          (let ((deg (length (gethash n adj nil))))
            (setq degrees (cons (cons n deg) degrees))
            (when (= 1 (% deg 2))
              (setq odd-count (1+ odd-count)))))
        (list
         (cond
          ((= odd-count 0) 'circuit)
          ((= odd-count 2) 'path)
          (t 'neither))
         odd-count
         (sort degrees (lambda (a b)
                         (string< (symbol-name (car a))
                                  (symbol-name (car b)))))))))

  (unwind-protect
      (let ((adj1 (make-hash-table :test 'eq))
            (adj2 (make-hash-table :test 'eq))
            (adj3 (make-hash-table :test 'eq)))
        ;; Euler circuit: square A-B-C-D-A (all degree 2)
        (puthash 'A '(B D) adj1)
        (puthash 'B '(A C) adj1)
        (puthash 'C '(B D) adj1)
        (puthash 'D '(C A) adj1)
        ;; Euler path: A-B-C-D (path graph, endpoints have degree 1)
        (puthash 'A '(B) adj2)
        (puthash 'B '(A C) adj2)
        (puthash 'C '(B D) adj2)
        (puthash 'D '(C) adj2)
        ;; Neither: K4 minus one edge = 4 odd-degree vertices
        (puthash 'A '(B C) adj3)
        (puthash 'B '(A C D) adj3)
        (puthash 'C '(A B D) adj3)
        (puthash 'D '(B C) adj3)
        (list
         (funcall 'neovm--ga-euler-check '(A B C D) adj1)
         (funcall 'neovm--ga-euler-check '(A B C D) adj2)
         (funcall 'neovm--ga-euler-check '(A B C D) adj3)))
    (fmakunbound 'neovm--ga-euler-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((circuit 0 ((A . 2) (B . 2) (C . 2) (D . 2))) (path 2 ((A . 1) (B . 2) (C . 2) (D . 1))) (path 2 ((A . 2) (B . 3) (C . 3) (D . 2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimum spanning tree (Prim's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_prim_mst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Prim's algorithm for MST. Weighted undirected graph.
    // Uses a list-based priority queue for simplicity.
    let form = r#"(progn
  (fset 'neovm--ga-prim-mst
    (lambda (nodes adj start)
      "Prim's MST. ADJ: node -> ((neighbor . weight) ...). Returns (total-weight edges)."
      (let ((in-tree (make-hash-table :test 'eq))
            (pq nil)  ;; (weight . (from . to))
            (total 0)
            (edges nil))
        (puthash start t in-tree)
        ;; Add edges from start
        (dolist (e (gethash start adj nil))
          (let ((entry (cons (cdr e) (cons start (car e)))))
            ;; Sorted insert
            (let ((new-pq nil) (inserted nil))
              (dolist (item pq)
                (when (and (not inserted) (< (car entry) (car item)))
                  (setq new-pq (cons entry new-pq))
                  (setq inserted t))
                (setq new-pq (cons item new-pq)))
              (unless inserted (setq new-pq (cons entry new-pq)))
              (setq pq (nreverse new-pq)))))
        (while pq
          (let* ((best (car pq))
                 (w (car best))
                 (from (cadr best))
                 (to (cddr best)))
            (setq pq (cdr pq))
            (unless (gethash to in-tree)
              (puthash to t in-tree)
              (setq total (+ total w))
              (setq edges (cons (list from to w) edges))
              ;; Add new edges from 'to'
              (dolist (e (gethash to adj nil))
                (unless (gethash (car e) in-tree)
                  (let ((entry (cons (cdr e) (cons to (car e)))))
                    (let ((new-pq nil) (inserted nil))
                      (dolist (item pq)
                        (when (and (not inserted) (< (car entry) (car item)))
                          (setq new-pq (cons entry new-pq))
                          (setq inserted t))
                        (setq new-pq (cons item new-pq)))
                      (unless inserted (setq new-pq (cons entry new-pq)))
                      (setq pq (nreverse new-pq)))))))))
        (list total (sort (nreverse edges)
                          (lambda (a b)
                            (or (string< (symbol-name (car a))
                                         (symbol-name (car b)))
                                (and (eq (car a) (car b))
                                     (string< (symbol-name (cadr a))
                                              (symbol-name (cadr b)))))))))))

  (unwind-protect
      (let ((adj (make-hash-table :test 'eq)))
        ;; Undirected weighted graph (add both directions):
        ;; A-B:4, A-C:2, B-C:1, B-D:5, C-D:8, C-E:10, D-E:2
        (puthash 'A '((B . 4) (C . 2)) adj)
        (puthash 'B '((A . 4) (C . 1) (D . 5)) adj)
        (puthash 'C '((A . 2) (B . 1) (D . 8) (E . 10)) adj)
        (puthash 'D '((B . 5) (C . 8) (E . 2)) adj)
        (puthash 'E '((C . 10) (D . 2)) adj)
        ;; MST from A: should have weight 4+1+2+2 = 9 or similar optimal
        (funcall 'neovm--ga-prim-mst '(A B C D E) adj 'A))
    (fmakunbound 'neovm--ga-prim-mst)))"#;
    let expect = expect_test::expect![[r#""OK (10 ((A C 2) (B D 5) (C B 1) (D E 2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle detection with path reporting (DFS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_cycle_detection_with_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFS-based cycle detection in a directed graph.
    // Returns nil if no cycle, or the cycle path if found.
    let form = r#"(progn
  (fset 'neovm--ga-find-cycle
    (lambda (nodes adj)
      "Find a cycle in directed graph. Returns cycle path or nil."
      (let ((white (make-hash-table :test 'eq))  ;; unvisited
            (gray (make-hash-table :test 'eq))   ;; in current DFS path
            (parent (make-hash-table :test 'eq))
            (cycle-found nil))
        ;; Mark all white
        (dolist (n nodes) (puthash n t white))
        (fset 'neovm--ga-cycle-dfs
          (lambda (u)
            (remhash u white)
            (puthash u t gray)
            (let ((found nil))
              (dolist (v (gethash u adj nil))
                (unless found
                  (cond
                   ((gethash v gray)
                    ;; Back edge => cycle found. Reconstruct.
                    (let ((path (list v u))
                          (cur u))
                      (while (not (eq cur v))
                        (setq cur (gethash cur parent))
                        (when cur (setq path (cons cur path))))
                      (setq cycle-found path)
                      (setq found t)))
                   ((gethash v white)
                    (puthash v u parent)
                    (funcall 'neovm--ga-cycle-dfs v)
                    (when cycle-found (setq found t))))))
              (remhash u gray))))
        ;; Run DFS from each unvisited node
        (dolist (n nodes)
          (when (and (gethash n white) (not cycle-found))
            (funcall 'neovm--ga-cycle-dfs n)))
        cycle-found)))

  (unwind-protect
      (let ((adj1 (make-hash-table :test 'eq))
            (adj2 (make-hash-table :test 'eq)))
        ;; DAG (no cycle): A->B, A->C, B->D, C->D
        (puthash 'A '(B C) adj1)
        (puthash 'B '(D) adj1)
        (puthash 'C '(D) adj1)
        (puthash 'D nil adj1)
        ;; Graph with cycle: A->B->C->D->B
        (puthash 'A '(B) adj2)
        (puthash 'B '(C) adj2)
        (puthash 'C '(D) adj2)
        (puthash 'D '(B) adj2)
        (list
         ;; No cycle
         (null (funcall 'neovm--ga-find-cycle '(A B C D) adj1))
         ;; Has cycle, verify it contains the cycle nodes
         (let ((cycle (funcall 'neovm--ga-find-cycle '(A B C D) adj2)))
           (list (not (null cycle))
                 ;; The cycle should contain B, C, D
                 (not (null (memq 'B cycle)))
                 (not (null (memq 'C cycle)))
                 (not (null (memq 'D cycle)))
                 (> (length cycle) 2)))))
    (fmakunbound 'neovm--ga-find-cycle)
    (fmakunbound 'neovm--ga-cycle-dfs)))"#;
    let expect = expect_test::expect![[r#""OK (t (t t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Floyd-Warshall all-pairs shortest path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_floyd_warshall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Floyd-Warshall computes shortest paths between all pairs.
    // Uses a 2D hash table (pair -> distance).
    let form = r#"(progn
  (fset 'neovm--ga-floyd-warshall
    (lambda (nodes edges)
      "All-pairs shortest path. EDGES: list of (from to weight)."
      (let ((dist (make-hash-table :test 'equal))
            (inf 99999))
        ;; Initialize
        (dolist (u nodes)
          (dolist (v nodes)
            (puthash (cons u v)
                     (if (eq u v) 0 inf)
                     dist)))
        ;; Set edge weights
        (dolist (e edges)
          (puthash (cons (nth 0 e) (nth 1 e)) (nth 2 e) dist))
        ;; Relax through each intermediate node
        (dolist (k nodes)
          (dolist (i nodes)
            (dolist (j nodes)
              (let ((through-k (+ (gethash (cons i k) dist)
                                  (gethash (cons k j) dist))))
                (when (< through-k (gethash (cons i j) dist))
                  (puthash (cons i j) through-k dist))))))
        dist)))

  (unwind-protect
      (let* ((nodes '(A B C D))
             (edges '((A B 3) (A C 8) (B C 1) (B D 7) (C D 2)))
             (result (funcall 'neovm--ga-floyd-warshall nodes edges)))
        ;; Extract interesting pairs
        (list
         (gethash '(A . A) result)
         (gethash '(A . B) result)
         (gethash '(A . C) result)
         (gethash '(A . D) result)
         (gethash '(B . C) result)
         (gethash '(B . D) result)
         (gethash '(C . D) result)
         ;; Verify triangle inequality: A->C <= A->B + B->C
         (<= (gethash '(A . C) result)
             (+ (gethash '(A . B) result)
                (gethash '(B . C) result)))))
    (fmakunbound 'neovm--ga-floyd-warshall)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 4 6 1 3 2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
