//! Complex combination oracle parity tests: graph shortest path algorithms
//! implemented in Elisp. Tests Dijkstra's algorithm, BFS shortest path
//! (unweighted), Bellman-Ford (negative edges), Floyd-Warshall (all-pairs),
//! path reconstruction, and cycle detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Dijkstra's algorithm: single-source shortest path on weighted graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_dijkstra() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dijkstra using a simple list-based priority queue (scan for min).
    // Returns distance map and predecessor map for path reconstruction.
    let form = r#"(progn
  ;; Graph as hash-table: node -> list of (neighbor . weight)
  (fset 'neovm--dj-add-edge
    (lambda (g u v w)
      (puthash u (cons (cons v w) (gethash u g nil)) g)))

  ;; Dijkstra: returns (distances . predecessors) hash-tables
  (fset 'neovm--dj-run
    (lambda (g src nodes)
      (let ((dist (make-hash-table))
            (pred (make-hash-table))
            (visited (make-hash-table))
            (unvisited (copy-sequence nodes)))
        ;; Initialize
        (dolist (n nodes)
          (puthash n 999999 dist))
        (puthash src 0 dist)
        (puthash src nil pred)
        ;; Main loop
        (while unvisited
          ;; Find unvisited node with smallest distance
          (let ((min-node nil) (min-dist 999999))
            (dolist (n unvisited)
              (when (< (gethash n dist) min-dist)
                (setq min-node n)
                (setq min-dist (gethash n dist))))
            (when (or (null min-node) (= min-dist 999999))
              (setq unvisited nil))  ;; no reachable nodes left
            (when min-node
              (setq unvisited (delq min-node unvisited))
              (puthash min-node t visited)
              ;; Relax edges
              (dolist (edge (gethash min-node g nil))
                (let ((neighbor (car edge))
                      (weight (cdr edge)))
                  (unless (gethash neighbor visited)
                    (let ((new-dist (+ min-dist weight)))
                      (when (< new-dist (gethash neighbor dist 999999))
                        (puthash neighbor new-dist dist)
                        (puthash neighbor min-node pred)))))))))
        (cons dist pred))))

  ;; Reconstruct path from pred table
  (fset 'neovm--dj-path
    (lambda (pred dst)
      (let ((path nil) (current dst))
        (while current
          (setq path (cons current path))
          (setq current (gethash current pred)))
        path)))

  (unwind-protect
      (let ((g (make-hash-table)))
        ;; Build weighted directed graph
        ;;   A --1--> B --2--> D
        ;;   |        |        ^
        ;;   4        1        |
        ;;   v        v        3
        ;;   C --5--> E --3--> D
        ;;   \                /
        ;;    `------7------>'
        (funcall 'neovm--dj-add-edge g 'A 'B 1)
        (funcall 'neovm--dj-add-edge g 'A 'C 4)
        (funcall 'neovm--dj-add-edge g 'B 'D 2)
        (funcall 'neovm--dj-add-edge g 'B 'E 1)
        (funcall 'neovm--dj-add-edge g 'C 'E 5)
        (funcall 'neovm--dj-add-edge g 'C 'D 7)
        (funcall 'neovm--dj-add-edge g 'E 'D 3)

        (let* ((result (funcall 'neovm--dj-run g 'A '(A B C D E)))
               (dist (car result))
               (pred (cdr result)))
          ;; Collect sorted (node . distance) pairs
          (let ((dist-list nil))
            (dolist (n '(A B C D E))
              (setq dist-list (cons (cons n (gethash n dist)) dist-list)))
            (list
              ;; Distances from A
              (sort (nreverse dist-list)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              ;; Shortest path A -> D
              (funcall 'neovm--dj-path pred 'D)
              ;; Shortest path A -> E
              (funcall 'neovm--dj-path pred 'E)
              ;; Distance A -> D should be 3 (A->B->D)
              (gethash 'D dist)
              ;; Distance A -> E should be 2 (A->B->E)
              (gethash 'E dist)))))
    (fmakunbound 'neovm--dj-add-edge)
    (fmakunbound 'neovm--dj-run)
    (fmakunbound 'neovm--dj-path)))"#;
    let expect = expect_test::expect![[
        r#""OK (((A . 0) (B . 1) (C . 4) (D . 3) (E . 2)) (A B D) (A B E) 3 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BFS shortest path: unweighted graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_bfs_unweighted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS on unweighted undirected graph: finds shortest path by edge count.
    // Returns distances and reconstructed paths for multiple targets.
    let form = r#"(let ((g (make-hash-table)))
      ;; Build undirected graph
      (dolist (edge '((1 2) (1 3) (2 4) (2 5) (3 6) (4 7) (5 7) (5 8) (6 8) (7 9) (8 9) (8 10) (9 10)))
        (let ((u (car edge)) (v (cadr edge)))
          (puthash u (cons v (gethash u g nil)) g)
          (puthash v (cons u (gethash v g nil)) g)))

      ;; BFS from source, returns (distances . predecessors)
      (let ((dist (make-hash-table))
            (pred (make-hash-table))
            (queue (list 1)))
        (puthash 1 0 dist)
        (puthash 1 nil pred)
        (while queue
          (let ((node (car queue)))
            (setq queue (cdr queue))
            (dolist (nb (gethash node g))
              (unless (gethash nb dist)
                (puthash nb (1+ (gethash node dist)) dist)
                (puthash nb node pred)
                (setq queue (append queue (list nb)))))))

        ;; Reconstruct path
        (let ((reconstruct
               (lambda (target)
                 (let ((path nil) (cur target))
                   (while cur
                     (setq path (cons cur path))
                     (setq cur (gethash cur pred)))
                   path))))

          ;; Collect all distances sorted
          (let ((all-dist nil))
            (maphash (lambda (k v) (setq all-dist (cons (cons k v) all-dist))) dist)
            (list
              ;; All distances from node 1
              (sort all-dist (lambda (a b) (< (car a) (car b))))
              ;; Path 1 -> 9
              (funcall reconstruct 9)
              ;; Path 1 -> 10
              (funcall reconstruct 10)
              ;; Distance to 10
              (gethash 10 dist)
              ;; Path length = distance + 1 (includes start)
              (= (length (funcall reconstruct 10))
                 (1+ (gethash 10 dist))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 0) (2 . 1) (3 . 1) (4 . 2) (5 . 2) (6 . 2) (7 . 3) (8 . 3) (9 . 4) (10 . 4)) (1 3 6 8 9) (1 3 6 8 10) 4 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bellman-Ford: handles negative edge weights, detects negative cycles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_bellman_ford() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Bellman-Ford: returns (distances predecessors has-negative-cycle)
  (fset 'neovm--bf-run
    (lambda (edges nodes src)
      (let ((dist (make-hash-table))
            (pred (make-hash-table))
            (n (length nodes)))
        ;; Initialize
        (dolist (node nodes)
          (puthash node 999999 dist))
        (puthash src 0 dist)
        ;; Relax all edges n-1 times
        (let ((i 0))
          (while (< i (1- n))
            (dolist (edge edges)
              (let ((u (nth 0 edge))
                    (v (nth 1 edge))
                    (w (nth 2 edge)))
                (when (< (+ (gethash u dist) w) (gethash v dist))
                  (puthash v (+ (gethash u dist) w) dist)
                  (puthash v u pred))))
            (setq i (1+ i))))
        ;; Check for negative cycles (one more pass)
        (let ((has-neg-cycle nil))
          (dolist (edge edges)
            (let ((u (nth 0 edge))
                  (v (nth 1 edge))
                  (w (nth 2 edge)))
              (when (< (+ (gethash u dist) w) (gethash v dist))
                (setq has-neg-cycle t))))
          (list dist pred has-neg-cycle)))))

  (fset 'neovm--bf-path
    (lambda (pred dst)
      (let ((path nil) (cur dst))
        (while cur
          (setq path (cons cur path))
          (setq cur (gethash cur pred)))
        path)))

  (unwind-protect
      (let* (;; Graph with negative edges but no negative cycle
             (edges1 '((A B 4) (A C 2) (B D 3) (B C -1) (C D 5) (C E 2) (D E -3) (E A 1)))
             (nodes1 '(A B C D E))
             (r1 (funcall 'neovm--bf-run edges1 nodes1 'A))
             (dist1 (nth 0 r1))
             (pred1 (nth 1 r1))
             (neg1 (nth 2 r1)))
        (let ((dist-list1 nil))
          (dolist (nd nodes1)
            (setq dist-list1 (cons (cons nd (gethash nd dist1)) dist-list1)))
          (let* (;; Graph WITH negative cycle: A->B(1), B->C(-3), C->A(1) => cycle weight = -1
                 (edges2 '((A B 1) (B C -3) (C A 1) (A D 5)))
                 (nodes2 '(A B C D))
                 (r2 (funcall 'neovm--bf-run edges2 nodes2 'A))
                 (neg2 (nth 2 r2)))
            (list
              ;; No negative cycle in graph 1
              neg1
              ;; Distances in graph 1
              (sort (nreverse dist-list1)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              ;; Path A -> E in graph 1
              (funcall 'neovm--bf-path pred1 'E)
              ;; Negative cycle detected in graph 2
              neg2))))
    (fmakunbound 'neovm--bf-run)
    (fmakunbound 'neovm--bf-path)))"#;
    let expect =
        expect_test::expect![[r#""OK (nil ((A . 0) (B . 4) (C . 2) (D . 7) (E . 4)) (A C E) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Floyd-Warshall: all-pairs shortest paths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_floyd_warshall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Floyd-Warshall using a 2D vector (matrix). Nodes mapped to indices.
    let form = r#"(progn
  (defvar neovm--fw-inf 999999)

  ;; Create n x n matrix filled with val
  (fset 'neovm--fw-matrix
    (lambda (n val)
      (let ((m (make-vector n nil)))
        (let ((i 0))
          (while (< i n)
            (aset m i (make-vector n val))
            (setq i (1+ i))))
        m)))

  ;; Get/set matrix element
  (fset 'neovm--fw-get (lambda (m i j) (aref (aref m i) j)))
  (fset 'neovm--fw-set (lambda (m i j v) (aset (aref m i) j v)))

  ;; Floyd-Warshall: takes edge list and node count, returns distance matrix
  (fset 'neovm--fw-run
    (lambda (edges n)
      (let ((dist (funcall 'neovm--fw-matrix n neovm--fw-inf))
            (next (funcall 'neovm--fw-matrix n -1)))
        ;; Self-distance = 0
        (let ((i 0))
          (while (< i n)
            (funcall 'neovm--fw-set dist i i 0)
            (funcall 'neovm--fw-set next i i i)
            (setq i (1+ i))))
        ;; Initialize from edges
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (w (nth 2 e)))
            (funcall 'neovm--fw-set dist u v w)
            (funcall 'neovm--fw-set next u v v)))
        ;; DP: try each intermediate node
        (let ((k 0))
          (while (< k n)
            (let ((i 0))
              (while (< i n)
                (let ((j 0))
                  (while (< j n)
                    (let ((through-k (+ (funcall 'neovm--fw-get dist i k)
                                        (funcall 'neovm--fw-get dist k j))))
                      (when (< through-k (funcall 'neovm--fw-get dist i j))
                        (funcall 'neovm--fw-set dist i j through-k)
                        (funcall 'neovm--fw-set next i j
                                 (funcall 'neovm--fw-get next i k))))
                    (setq j (1+ j))))
                (setq i (1+ i))))
            (setq k (1+ k))))
        (cons dist next))))

  ;; Reconstruct path using next matrix
  (fset 'neovm--fw-path
    (lambda (next u v)
      (if (= (funcall 'neovm--fw-get next u v) -1)
          nil
        (let ((path (list u)) (cur u))
          (while (/= cur v)
            (setq cur (funcall 'neovm--fw-get next cur v))
            (setq path (cons cur path)))
          (nreverse path)))))

  (unwind-protect
      (let* (;; 5 nodes (0-4), weighted directed edges
             ;; 0->1(3), 0->2(8), 1->3(1), 1->4(7), 2->1(4), 3->0(2), 3->2(2), 4->3(1)
             (edges '((0 1 3) (0 2 8) (1 3 1) (1 4 7) (2 1 4) (3 0 2) (3 2 2) (4 3 1)))
             (result (funcall 'neovm--fw-run edges 5))
             (dist (car result))
             (next (cdr result)))
        ;; Collect all-pairs distances
        (let ((all-dists nil))
          (let ((i 0))
            (while (< i 5)
              (let ((j 0))
                (while (< j 5)
                  (let ((d (funcall 'neovm--fw-get dist i j)))
                    (when (< d neovm--fw-inf)
                      (setq all-dists (cons (list i j d) all-dists))))
                  (setq j (1+ j))))
              (setq i (1+ i))))
          (list
            ;; All reachable pairs with distances, sorted
            (sort (nreverse all-dists)
                  (lambda (a b)
                    (or (< (car a) (car b))
                        (and (= (car a) (car b))
                             (< (cadr a) (cadr b))))))
            ;; Path 0 -> 4
            (funcall 'neovm--fw-path next 0 4)
            ;; Distance 0 -> 4
            (funcall 'neovm--fw-get dist 0 4)
            ;; Path 4 -> 2
            (funcall 'neovm--fw-path next 4 2)
            ;; Distance 4 -> 2
            (funcall 'neovm--fw-get dist 4 2)
            ;; Verify symmetry of self-distances = 0
            (let ((ok t) (i 0))
              (while (< i 5)
                (unless (= 0 (funcall 'neovm--fw-get dist i i))
                  (setq ok nil))
                (setq i (1+ i)))
              ok))))
    (fmakunbound 'neovm--fw-matrix)
    (fmakunbound 'neovm--fw-get)
    (fmakunbound 'neovm--fw-set)
    (fmakunbound 'neovm--fw-run)
    (fmakunbound 'neovm--fw-path)
    (makunbound 'neovm--fw-inf)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 0) (0 1 3) (0 2 6) (0 3 4) (0 4 10) (1 0 3) (1 1 0) (1 2 3) (1 3 1) (1 4 7) (2 0 7) (2 1 4) (2 2 0) (2 3 5) (2 4 11) (3 0 2) (3 1 5) (3 2 2) (3 3 0) (3 4 12) (4 0 3) (4 1 6) (4 2 3) (4 3 1) (4 4 0)) (0 1 4) 10 (4 3 2) 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Path reconstruction: Dijkstra with full path output on larger graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_path_reconstruction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Larger graph with multiple shortest-path queries and full path reconstruction.
    let form = r#"(progn
  (fset 'neovm--pr-add
    (lambda (g u v w)
      (puthash u (cons (cons v w) (gethash u g nil)) g)
      (puthash v (cons (cons u w) (gethash v g nil)) g)))

  (fset 'neovm--pr-dijkstra
    (lambda (g src nodes)
      (let ((dist (make-hash-table))
            (pred (make-hash-table))
            (visited (make-hash-table))
            (todo (copy-sequence nodes)))
        (dolist (n nodes) (puthash n 999999 dist))
        (puthash src 0 dist)
        (while todo
          (let ((best nil) (best-d 999999))
            (dolist (n todo)
              (when (< (gethash n dist) best-d)
                (setq best n best-d (gethash n dist))))
            (if (or (null best) (= best-d 999999))
                (setq todo nil)
              (setq todo (delq best todo))
              (puthash best t visited)
              (dolist (edge (gethash best g nil))
                (let ((nb (car edge)) (w (cdr edge)))
                  (unless (gethash nb visited)
                    (let ((nd (+ best-d w)))
                      (when (< nd (gethash nb dist 999999))
                        (puthash nb nd dist)
                        (puthash nb best pred)))))))))
        (cons dist pred))))

  (fset 'neovm--pr-path
    (lambda (pred target)
      (let ((path nil) (cur target))
        (while cur
          (setq path (cons cur path))
          (setq cur (gethash cur pred)))
        path)))

  (unwind-protect
      (let ((g (make-hash-table))
            (nodes '(A B C D E F G H)))
        ;; Grid-like graph with varying weights
        (funcall 'neovm--pr-add g 'A 'B 1)
        (funcall 'neovm--pr-add g 'A 'C 4)
        (funcall 'neovm--pr-add g 'B 'C 2)
        (funcall 'neovm--pr-add g 'B 'D 5)
        (funcall 'neovm--pr-add g 'C 'D 1)
        (funcall 'neovm--pr-add g 'C 'E 3)
        (funcall 'neovm--pr-add g 'D 'F 2)
        (funcall 'neovm--pr-add g 'E 'F 1)
        (funcall 'neovm--pr-add g 'E 'G 6)
        (funcall 'neovm--pr-add g 'F 'G 3)
        (funcall 'neovm--pr-add g 'F 'H 4)
        (funcall 'neovm--pr-add g 'G 'H 1)

        (let* ((r (funcall 'neovm--pr-dijkstra g 'A nodes))
               (dist (car r))
               (pred (cdr r)))
          ;; Collect all distances
          (let ((dists nil))
            (dolist (n nodes)
              (setq dists (cons (cons n (gethash n dist)) dists)))
            (list
              ;; Sorted distances from A
              (sort (nreverse dists)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              ;; Path A -> H
              (funcall 'neovm--pr-path pred 'H)
              ;; Distance A -> H
              (gethash 'H dist)
              ;; Path A -> G
              (funcall 'neovm--pr-path pred 'G)
              ;; Distance A -> G
              (gethash 'G dist)
              ;; Path A -> F
              (funcall 'neovm--pr-path pred 'F)
              ;; Verify path costs match distances
              (let ((path-to-h (funcall 'neovm--pr-path pred 'H)))
                ;; Sum edge weights along path
                (let ((total 0) (i 0))
                  (while (< i (1- (length path-to-h)))
                    (let* ((u (nth i path-to-h))
                           (v (nth (1+ i) path-to-h))
                           (w nil))
                      (dolist (edge (gethash u g))
                        (when (eq (car edge) v)
                          (setq w (cdr edge))))
                      (setq total (+ total w)))
                    (setq i (1+ i)))
                  (= total (gethash 'H dist))))))))
    (fmakunbound 'neovm--pr-add)
    (fmakunbound 'neovm--pr-dijkstra)
    (fmakunbound 'neovm--pr-path)))"#;
    let expect = expect_test::expect![[
        r#""OK (((A . 0) (B . 1) (C . 3) (D . 4) (E . 6) (F . 6) (G . 9) (H . 10)) (A B C D F H) 10 (A B C D F G) 9 (A B C D F) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle detection: DFS-based with back-edge identification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_cycle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFS-based cycle detection on directed graphs using tri-coloring
    // (0=white/unvisited, 1=gray/in-progress, 2=black/done).
    // A back edge (to gray node) indicates a cycle.
    let form = r#"(progn
  (fset 'neovm--cd-detect
    (lambda (g nodes)
      (let ((color (make-hash-table))
            (cycles nil))
        (dolist (n nodes) (puthash n 0 color))
        ;; DFS visit with cycle back-edge recording
        (fset 'neovm--cd-visit
          (lambda (node path)
            (puthash node 1 color)
            (dolist (nb (gethash node g nil))
              (cond
                ((= (gethash nb color 0) 1)
                 ;; Back edge: extract cycle from path
                 (let ((cycle nil) (found nil)
                       (rpath (reverse (cons node path))))
                   (dolist (p rpath)
                     (when (eq p nb) (setq found t))
                     (when found (setq cycle (cons p cycle))))
                   (when cycle
                     (setq cycles (cons cycle cycles)))))
                ((= (gethash nb color 0) 0)
                 (funcall 'neovm--cd-visit nb (cons node path)))))
            (puthash node 2 color)))

        (dolist (n nodes)
          (when (= (gethash n color) 0)
            (funcall 'neovm--cd-visit n nil)))
        (list (not (null cycles)) (nreverse cycles)))))

  (unwind-protect
      (let ((g1 (make-hash-table))
            (g2 (make-hash-table))
            (g3 (make-hash-table)))
        ;; g1: DAG (no cycle)
        (puthash 'A '(B C) g1)
        (puthash 'B '(D) g1)
        (puthash 'C '(D E) g1)
        (puthash 'D '(F) g1)
        (puthash 'E '(F) g1)
        (puthash 'F nil g1)

        ;; g2: single cycle A->B->C->A
        (puthash 'A '(B) g2)
        (puthash 'B '(C) g2)
        (puthash 'C '(A D) g2)
        (puthash 'D nil g2)

        ;; g3: two independent cycles: A->B->A and C->D->E->C
        (puthash 'A '(B) g3)
        (puthash 'B '(A) g3)
        (puthash 'C '(D) g3)
        (puthash 'D '(E) g3)
        (puthash 'E '(C) g3)

        (let ((r1 (funcall 'neovm--cd-detect g1 '(A B C D E F)))
              (r2 (funcall 'neovm--cd-detect g2 '(A B C D)))
              (r3 (funcall 'neovm--cd-detect g3 '(A B C D E))))
          (list
            ;; g1: no cycle
            (car r1)
            ;; g2: has cycle
            (car r2)
            ;; g3: has cycle(s)
            (car r3)
            ;; g3: should have found 2 cycles
            (length (cadr r3)))))
    (fmakunbound 'neovm--cd-detect)
    (fmakunbound 'neovm--cd-visit)))"#;
    let expect = expect_test::expect![[r#""OK (nil t t 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dijkstra on disconnected graph: unreachable nodes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_sp_dijkstra_disconnected() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test Dijkstra on a graph where some nodes are unreachable from the source.
    let form = r#"(progn
  (fset 'neovm--dd-add
    (lambda (g u v w)
      (puthash u (cons (cons v w) (gethash u g nil)) g)))

  (fset 'neovm--dd-dijkstra
    (lambda (g src nodes)
      (let ((dist (make-hash-table))
            (pred (make-hash-table))
            (visited (make-hash-table))
            (todo (copy-sequence nodes)))
        (dolist (n nodes) (puthash n 999999 dist))
        (puthash src 0 dist)
        (while todo
          (let ((best nil) (best-d 999999))
            (dolist (n todo)
              (when (< (gethash n dist) best-d)
                (setq best n best-d (gethash n dist))))
            (if (or (null best) (= best-d 999999))
                (setq todo nil)
              (setq todo (delq best todo))
              (puthash best t visited)
              (dolist (edge (gethash best g nil))
                (let ((nb (car edge)) (w (cdr edge)))
                  (unless (gethash nb visited)
                    (let ((nd (+ best-d w)))
                      (when (< nd (gethash nb dist 999999))
                        (puthash nb nd dist)
                        (puthash nb best pred)))))))))
        (cons dist pred))))

  (unwind-protect
      (let ((g (make-hash-table))
            (nodes '(A B C D E F)))
        ;; Component 1: A -> B -> C
        (funcall 'neovm--dd-add g 'A 'B 2)
        (funcall 'neovm--dd-add g 'B 'C 3)
        ;; Component 2: D -> E -> F (disconnected from A)
        (funcall 'neovm--dd-add g 'D 'E 1)
        (funcall 'neovm--dd-add g 'E 'F 4)

        (let* ((r (funcall 'neovm--dd-dijkstra g 'A nodes))
               (dist (car r)))
          (let ((reachable nil) (unreachable nil))
            (dolist (n nodes)
              (if (< (gethash n dist) 999999)
                  (setq reachable (cons (cons n (gethash n dist)) reachable))
                (setq unreachable (cons n unreachable))))
            (list
              ;; Reachable nodes with distances
              (sort (nreverse reachable)
                    (lambda (a b) (string< (symbol-name (car a))
                                           (symbol-name (car b)))))
              ;; Unreachable nodes
              (sort (nreverse unreachable)
                    (lambda (a b) (string< (symbol-name a)
                                           (symbol-name b))))
              ;; Specific distances
              (gethash 'A dist)
              (gethash 'B dist)
              (gethash 'C dist)
              ;; D is unreachable
              (= (gethash 'D dist) 999999)))))
    (fmakunbound 'neovm--dd-add)
    (fmakunbound 'neovm--dd-dijkstra)))"#;
    let expect = expect_test::expect![[r#""OK (((A . 0) (B . 2) (C . 5)) (D E F) 0 2 5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
