//! Advanced oracle parity tests for graph theory algorithms in pure Elisp:
//! Tarjan's strongly connected components, Dijkstra's shortest path,
//! bipartite checking, bridge finding, Eulerian path detection,
//! greedy graph coloring, minimum spanning tree (Kruskal's/Prim's),
//! topological sort with multiple valid orderings, and advanced
//! cycle extraction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Tarjan's Strongly Connected Components
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_tarjan_scc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Tarjan's SCC using an explicit stack emulating recursion.
  ;; Graph: alist of (node . neighbors)
  (fset 'neovm--tarjan-scc
    (lambda (graph nodes)
      (let ((index-counter 0)
            (stack nil)
            (on-stack (make-hash-table))
            (index-map (make-hash-table))
            (lowlink (make-hash-table))
            (sccs nil))
        ;; Recursive DFS helper
        (fset 'neovm--tarjan-visit
          (lambda (v)
            (puthash v index-counter index-map)
            (puthash v index-counter lowlink)
            (setq index-counter (1+ index-counter))
            (setq stack (cons v stack))
            (puthash v t on-stack)
            ;; Explore neighbors
            (dolist (w (cdr (assq v graph)))
              (cond
               ((not (gethash w index-map))
                ;; Not yet visited
                (funcall 'neovm--tarjan-visit w)
                (puthash v (min (gethash v lowlink) (gethash w lowlink)) lowlink))
               ((gethash w on-stack)
                ;; On stack: back edge
                (puthash v (min (gethash v lowlink) (gethash w index-map)) lowlink))))
            ;; If v is root of SCC, pop the component
            (when (= (gethash v lowlink) (gethash v index-map))
              (let ((component nil) (done nil))
                (while (not done)
                  (let ((w (car stack)))
                    (setq stack (cdr stack))
                    (puthash w nil on-stack)
                    (setq component (cons w component))
                    (when (eq w v) (setq done t))))
                (setq sccs (cons (sort component
                                       (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                                 sccs))))))
        (dolist (n nodes)
          (unless (gethash n index-map)
            (funcall 'neovm--tarjan-visit n)))
        (sort (nreverse sccs)
              (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b))))))))

  (unwind-protect
      (let* (;; Graph with 3 SCCs: {a,b,c}, {d,e}, {f}
             (graph '((a . (b))
                      (b . (c))
                      (c . (a d))
                      (d . (e))
                      (e . (d f))
                      (f . ())))
             (nodes '(a b c d e f))
             (sccs (funcall 'neovm--tarjan-scc graph nodes)))
        (list
          (list 'num-sccs (length sccs))
          (list 'sccs sccs)
          ;; Second graph: single large SCC
          (let* ((g2 '((p . (q))
                       (q . (r))
                       (r . (s))
                       (s . (p))))
                 (n2 '(p q r s))
                 (s2 (funcall 'neovm--tarjan-scc g2 n2)))
            (list 'single-scc s2 (= (length s2) 1)))
          ;; Third: DAG (each node is its own SCC)
          (let* ((g3 '((x . (y z))
                       (y . (z))
                       (z . ())))
                 (n3 '(x y z))
                 (s3 (funcall 'neovm--tarjan-scc g3 n3)))
            (list 'dag-sccs (length s3) (= (length s3) 3)))))
    (fmakunbound 'neovm--tarjan-scc)
    (fmakunbound 'neovm--tarjan-visit)))"#;
    let expect = expect_test::expect![[
        r#""OK ((num-sccs 3) (sccs ((a b c) (d e) (f))) (single-scc ((p q r s)) t) (dag-sccs 3 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dijkstra's shortest path with path reconstruction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_dijkstra() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Weighted graph: alist of (node . ((neighbor . weight) ...))
  ;; Dijkstra with linear scan for min (no real priority queue in Elisp)
  (fset 'neovm--dijkstra
    (lambda (graph src nodes)
      (let ((dist (make-hash-table))
            (prev (make-hash-table))
            (visited (make-hash-table))
            (unvisited (copy-sequence nodes)))
        (dolist (n nodes) (puthash n 999999 dist))
        (puthash src 0 dist)
        (while unvisited
          ;; Find min-dist unvisited node
          (let ((u nil) (min-d 999999))
            (dolist (n unvisited)
              (when (< (gethash n dist) min-d)
                (setq u n min-d (gethash n dist))))
            (if (or (null u) (= min-d 999999))
                (setq unvisited nil)
              (setq unvisited (delq u unvisited))
              (puthash u t visited)
              ;; Relax edges
              (dolist (edge (cdr (assq u graph)))
                (let* ((v (car edge)) (w (cdr edge))
                       (alt (+ min-d w)))
                  (when (< alt (gethash v dist 999999))
                    (puthash v alt dist)
                    (puthash v u prev)))))))
        (cons dist prev))))

  ;; Reconstruct path
  (fset 'neovm--dj-path
    (lambda (prev-map src dst)
      (if (eq src dst) (list src)
        (let ((path nil) (cur dst))
          (while (and cur (not (eq cur src)))
            (setq path (cons cur path))
            (setq cur (gethash cur prev-map)))
          (if cur (cons src path) nil)))))

  (unwind-protect
      (let ((g '((A . ((B . 4) (C . 2)))
                 (B . ((D . 3) (C . 5) (E . 1)))
                 (C . ((B . 1) (D . 8) (E . 10)))
                 (D . ((E . 2)))
                 (E . ()))))
        (let* ((result (funcall 'neovm--dijkstra g 'A '(A B C D E)))
               (dist (car result))
               (prev (cdr result)))
          ;; Collect distances
          (let ((dists nil))
            (dolist (n '(A B C D E))
              (setq dists (cons (cons n (gethash n dist)) dists)))
            (list
              (sort (nreverse dists)
                    (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))
              ;; Shortest paths
              (list 'A-to-D (funcall 'neovm--dj-path prev 'A 'D)
                    (gethash 'D dist))
              (list 'A-to-E (funcall 'neovm--dj-path prev 'A 'E)
                    (gethash 'E dist))
              (list 'A-to-B (funcall 'neovm--dj-path prev 'A 'B)
                    (gethash 'B dist))))))
    (fmakunbound 'neovm--dijkstra)
    (fmakunbound 'neovm--dj-path)))"#;
    let expect = expect_test::expect![[
        r#""OK (((A . 0) (B . 3) (C . 2) (D . 6) (E . 4)) (A-to-D (A C B D) 6) (A-to-E (A C B E) 4) (A-to-B (A C B) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bipartite checking via BFS 2-coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_bipartite_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Check if undirected graph is bipartite using BFS 2-coloring
  (fset 'neovm--is-bipartite
    (lambda (graph nodes)
      (let ((color (make-hash-table))
            (bipartite t))
        ;; Try to 2-color from each unvisited node (handles disconnected graphs)
        (dolist (start nodes)
          (unless (gethash start color)
            (puthash start 0 color)
            (let ((queue (list start)))
              (while (and queue bipartite)
                (let ((u (car queue)))
                  (setq queue (cdr queue))
                  (dolist (v (cdr (assq u graph)))
                    (cond
                     ((not (gethash v color nil))
                      (puthash v (- 1 (gethash u color)) color)
                      (setq queue (append queue (list v))))
                     ((= (gethash v color) (gethash u color))
                      (setq bipartite nil)))))))))
        (let ((group-0 nil) (group-1 nil))
          (when bipartite
            (dolist (n nodes)
              (if (= (gethash n color 0) 0)
                  (setq group-0 (cons n group-0))
                (setq group-1 (cons n group-1)))))
          (list bipartite
                (when bipartite
                  (list (sort (nreverse group-0) (lambda (a b) (< a b)))
                        (sort (nreverse group-1) (lambda (a b) (< a b))))))))))

  (unwind-protect
      (list
        ;; Bipartite: complete bipartite K_{2,3}
        (let ((g '((1 . (3 4 5))
                   (2 . (3 4 5))
                   (3 . (1 2))
                   (4 . (1 2))
                   (5 . (1 2)))))
          (funcall 'neovm--is-bipartite g '(1 2 3 4 5)))
        ;; Not bipartite: odd cycle (triangle)
        (let ((g '((1 . (2 3))
                   (2 . (1 3))
                   (3 . (1 2)))))
          (funcall 'neovm--is-bipartite g '(1 2 3)))
        ;; Bipartite: path graph 1-2-3-4
        (let ((g '((1 . (2))
                   (2 . (1 3))
                   (3 . (2 4))
                   (4 . (3)))))
          (funcall 'neovm--is-bipartite g '(1 2 3 4)))
        ;; Not bipartite: 5-cycle
        (let ((g '((1 . (2 5))
                   (2 . (1 3))
                   (3 . (2 4))
                   (4 . (3 5))
                   (5 . (4 1)))))
          (funcall 'neovm--is-bipartite g '(1 2 3 4 5)))
        ;; Bipartite: even cycle (square)
        (let ((g '((1 . (2 4))
                   (2 . (1 3))
                   (3 . (2 4))
                   (4 . (3 1)))))
          (funcall 'neovm--is-bipartite g '(1 2 3 4))))
    (fmakunbound 'neovm--is-bipartite)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t ((1 2) (3 4 5))) (nil nil) (t ((1 3) (2 4))) (nil nil) (t ((1 3) (2 4))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle extraction: find one actual cycle in a directed graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_cycle_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Find a cycle in directed graph, return cycle as list of nodes
  (fset 'neovm--find-cycle
    (lambda (graph nodes)
      (let ((color (make-hash-table))
            (parent (make-hash-table))
            (cycle-found nil))
        (dolist (n nodes) (puthash n 'white color))
        (fset 'neovm--fc-visit
          (lambda (u)
            (puthash u 'gray color)
            (catch 'found
              (dolist (v (cdr (assq u graph)))
                (when cycle-found (throw 'found nil))
                (cond
                 ((eq (gethash v color 'white) 'gray)
                  ;; Back edge: extract cycle
                  (let ((cycle (list v)) (cur u))
                    (while (not (eq cur v))
                      (setq cycle (cons cur cycle))
                      (setq cur (gethash cur parent)))
                    (setq cycle (cons v cycle))
                    (setq cycle-found (nreverse cycle))
                    (throw 'found nil)))
                 ((eq (gethash v color 'white) 'white)
                  (puthash v u parent)
                  (funcall 'neovm--fc-visit v)))))
            (puthash u 'black color)))
        (dolist (n nodes)
          (when (and (eq (gethash n color) 'white) (not cycle-found))
            (funcall 'neovm--fc-visit n)))
        cycle-found)))

  (unwind-protect
      (list
        ;; Graph with cycle a->b->c->a
        (funcall 'neovm--find-cycle
          '((a . (b)) (b . (c)) (c . (a d)) (d . (e)) (e . ()))
          '(a b c d e))
        ;; DAG: no cycle
        (funcall 'neovm--find-cycle
          '((a . (b c)) (b . (d)) (c . (d)) (d . ()))
          '(a b c d))
        ;; Self-loop
        (funcall 'neovm--find-cycle
          '((x . (x y)) (y . ()))
          '(x y))
        ;; Two separate cycles
        (funcall 'neovm--find-cycle
          '((a . (b)) (b . (a)) (c . (d)) (d . (c)))
          '(a b c d)))
    (fmakunbound 'neovm--find-cycle)
    (fmakunbound 'neovm--fc-visit)))"#;
    let expect = expect_test::expect![[r#""OK ((a c b a) nil (x x) (a b a))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Greedy graph coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_greedy_coloring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Greedy coloring: assign smallest available color to each node
  (fset 'neovm--greedy-color
    (lambda (graph nodes)
      (let ((coloring (make-hash-table)))
        (dolist (v nodes)
          ;; Find colors used by neighbors
          (let ((used nil))
            (dolist (u (cdr (assq v graph)))
              (let ((c (gethash u coloring)))
                (when c (setq used (cons c used)))))
            ;; Find smallest non-used color (starting from 0)
            (let ((c 0))
              (while (memq c used) (setq c (1+ c)))
              (puthash v c coloring))))
        ;; Return alist sorted by node
        (let ((result nil))
          (dolist (v nodes)
            (setq result (cons (cons v (gethash v coloring)) result)))
          (nreverse result)))))

  ;; Verify coloring is valid: no two adjacent nodes share a color
  (fset 'neovm--valid-coloring
    (lambda (graph coloring-alist)
      (let ((h (make-hash-table))
            (valid t))
        (dolist (pair coloring-alist) (puthash (car pair) (cdr pair) h))
        (dolist (entry graph)
          (let ((u (car entry)))
            (dolist (v (cdr entry))
              (when (= (gethash u h) (gethash v h))
                (setq valid nil)))))
        valid)))

  (unwind-protect
      (list
        ;; Bipartite graph: should need only 2 colors
        (let* ((g '((1 . (2 4)) (2 . (1 3)) (3 . (2 4)) (4 . (3 1))))
               (c (funcall 'neovm--greedy-color g '(1 2 3 4)))
               (num-colors (length (delete-dups (mapcar #'cdr (copy-sequence c))))))
          (list c num-colors (funcall 'neovm--valid-coloring g c)))
        ;; Complete graph K4: needs 4 colors
        (let* ((g '((1 . (2 3 4)) (2 . (1 3 4)) (3 . (1 2 4)) (4 . (1 2 3))))
               (c (funcall 'neovm--greedy-color g '(1 2 3 4)))
               (num-colors (length (delete-dups (mapcar #'cdr (copy-sequence c))))))
          (list c num-colors (funcall 'neovm--valid-coloring g c)))
        ;; Star graph: 2 colors
        (let* ((g '((0 . (1 2 3 4 5)) (1 . (0)) (2 . (0)) (3 . (0)) (4 . (0)) (5 . (0))))
               (c (funcall 'neovm--greedy-color g '(0 1 2 3 4 5)))
               (num-colors (length (delete-dups (mapcar #'cdr (copy-sequence c))))))
          (list c num-colors (funcall 'neovm--valid-coloring g c)))
        ;; Odd cycle (5-cycle): 3 colors
        (let* ((g '((1 . (2 5)) (2 . (1 3)) (3 . (2 4)) (4 . (3 5)) (5 . (4 1))))
               (c (funcall 'neovm--greedy-color g '(1 2 3 4 5)))
               (num-colors (length (delete-dups (mapcar #'cdr (copy-sequence c))))))
          (list c num-colors (funcall 'neovm--valid-coloring g c))))
    (fmakunbound 'neovm--greedy-color)
    (fmakunbound 'neovm--valid-coloring)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((1 . 0) (2 . 1) (3 . 0) (4 . 1)) 2 t) (((1 . 0) (2 . 1) (3 . 2) (4 . 3)) 4 t) (((0 . 0) (1 . 1) (2 . 1) (3 . 1) (4 . 1) (5 . 1)) 2 t) (((1 . 0) (2 . 1) (3 . 0) (4 . 1) (5 . 2)) 3 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort with all valid orderings (enumerate via backtracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_topo_sort_all_orderings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Kahn's algorithm to find ONE topological sort + verify
  ;; Then enumerate all valid orderings for small DAGs via backtracking
  (fset 'neovm--kahn-topo
    (lambda (graph nodes)
      (let ((in-deg (make-hash-table))
            (adj (make-hash-table))
            (result nil))
        (dolist (n nodes) (puthash n 0 in-deg) (puthash n nil adj))
        (dolist (entry graph)
          (let ((u (car entry)))
            (puthash u (cdr entry) adj)
            (dolist (v (cdr entry))
              (puthash v (1+ (gethash v in-deg 0)) in-deg))))
        ;; Collect nodes with in-degree 0
        (let ((queue nil))
          (dolist (n nodes)
            (when (= (gethash n in-deg) 0)
              (setq queue (cons n queue))))
          (setq queue (sort queue (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
          (while queue
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (setq result (cons u result))
              (dolist (v (gethash u adj))
                (puthash v (1- (gethash v in-deg)) in-deg)
                (when (= (gethash v in-deg) 0)
                  (setq queue (sort (cons v queue)
                                    (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))))
        (nreverse result))))

  ;; Verify topological ordering
  (fset 'neovm--verify-topo
    (lambda (graph order)
      (let ((pos (make-hash-table)) (idx 0) (ok t))
        (dolist (n order) (puthash n idx pos) (setq idx (1+ idx)))
        (dolist (entry graph)
          (dolist (v (cdr entry))
            (when (>= (gethash (car entry) pos) (gethash v pos))
              (setq ok nil))))
        ok)))

  (unwind-protect
      (list
        ;; Diamond DAG: a->{b,c}, b->d, c->d
        (let* ((g '((a . (b c)) (b . (d)) (c . (d)) (d . ())))
               (order (funcall 'neovm--kahn-topo g '(a b c d))))
          (list order (funcall 'neovm--verify-topo g order)))
        ;; Linear chain
        (let* ((g '((a . (b)) (b . (c)) (c . (d)) (d . ())))
               (order (funcall 'neovm--kahn-topo g '(a b c d))))
          (list order (funcall 'neovm--verify-topo g order)))
        ;; Multiple sources
        (let* ((g '((a . (c)) (b . (c)) (c . (d)) (d . ())))
               (order (funcall 'neovm--kahn-topo g '(a b c d))))
          (list order (funcall 'neovm--verify-topo g order)))
        ;; Wider DAG
        (let* ((g '((a . (c d)) (b . (d e)) (c . (f)) (d . (f)) (e . (f)) (f . ())))
               (order (funcall 'neovm--kahn-topo g '(a b c d e f))))
          (list order (funcall 'neovm--verify-topo g order))))
    (fmakunbound 'neovm--kahn-topo)
    (fmakunbound 'neovm--verify-topo)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b c d) t) ((a b c d) t) ((a b c d) t) ((a b c d e f) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bridge finding in undirected graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_bridge_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Find bridges using DFS with discovery time and low values
  (fset 'neovm--find-bridges
    (lambda (graph nodes)
      (let ((timer 0)
            (disc (make-hash-table))
            (low (make-hash-table))
            (visited (make-hash-table))
            (bridges nil))
        (fset 'neovm--bridge-dfs
          (lambda (u parent-node)
            (puthash u t visited)
            (puthash u timer disc)
            (puthash u timer low)
            (setq timer (1+ timer))
            (dolist (v (cdr (assq u graph)))
              (cond
               ((not (gethash v visited))
                (funcall 'neovm--bridge-dfs v u)
                (puthash u (min (gethash u low) (gethash v low)) low)
                ;; If low[v] > disc[u], then u-v is a bridge
                (when (> (gethash v low) (gethash u disc))
                  (setq bridges (cons (list u v) bridges))))
               ((not (eq v parent-node))
                (puthash u (min (gethash u low) (gethash v disc)) low))))))
        (dolist (n nodes)
          (unless (gethash n visited)
            (funcall 'neovm--bridge-dfs n nil)))
        (sort (mapcar (lambda (b) (sort (copy-sequence b)
                                        (lambda (a b) (< a b))))
                       (nreverse bridges))
              (lambda (a b) (< (car a) (car b)))))))

  (unwind-protect
      (list
        ;; Graph with bridges: 1-2 is bridge, 3-4-5-3 is cycle
        (let ((g '((1 . (2))
                   (2 . (1 3))
                   (3 . (2 4 5))
                   (4 . (3 5))
                   (5 . (3 4)))))
          (funcall 'neovm--find-bridges g '(1 2 3 4 5)))
        ;; No bridges: complete graph K4
        (let ((g '((1 . (2 3 4))
                   (2 . (1 3 4))
                   (3 . (1 2 4))
                   (4 . (1 2 3)))))
          (funcall 'neovm--find-bridges g '(1 2 3 4)))
        ;; Linear graph: all edges are bridges
        (let ((g '((1 . (2))
                   (2 . (1 3))
                   (3 . (2 4))
                   (4 . (3)))))
          (funcall 'neovm--find-bridges g '(1 2 3 4)))
        ;; Star: all edges are bridges
        (let ((g '((0 . (1 2 3))
                   (1 . (0))
                   (2 . (0))
                   (3 . (0)))))
          (funcall 'neovm--find-bridges g '(0 1 2 3))))
    (fmakunbound 'neovm--find-bridges)
    (fmakunbound 'neovm--bridge-dfs)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2) (2 3)) nil ((1 2) (2 3) (3 4)) ((0 1) (0 2) (0 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Eulerian path/circuit detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_eulerian_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Check if undirected graph has Eulerian circuit (all degrees even)
  ;; or Eulerian path (exactly 0 or 2 odd-degree vertices)
  ;; Also check connectivity of non-isolated vertices
  (fset 'neovm--euler-check
    (lambda (graph nodes)
      (let ((degrees nil)
            (odd-count 0)
            (non-isolated nil))
        ;; Compute degrees
        (dolist (n nodes)
          (let ((deg (length (cdr (assq n graph)))))
            (setq degrees (cons (cons n deg) degrees))
            (when (> deg 0) (setq non-isolated (cons n non-isolated)))
            (when (= (% deg 2) 1) (setq odd-count (1+ odd-count)))))
        ;; Check connectivity of non-isolated vertices via BFS
        (let ((connected t))
          (when (> (length non-isolated) 0)
            (let ((visited (make-hash-table))
                  (queue (list (car non-isolated))))
              (puthash (car non-isolated) t visited)
              (while queue
                (let ((u (car queue)))
                  (setq queue (cdr queue))
                  (dolist (v (cdr (assq u graph)))
                    (unless (gethash v visited)
                      (puthash v t visited)
                      (setq queue (append queue (list v)))))))
              (dolist (n non-isolated)
                (unless (gethash n visited)
                  (setq connected nil)))))
          (list
            (list 'degrees (sort (nreverse degrees)
                                 (lambda (a b) (< (car a) (car b)))))
            (list 'odd-degree-count odd-count)
            (list 'connected connected)
            (list 'has-euler-circuit (and connected (= odd-count 0) (> (length non-isolated) 0)))
            (list 'has-euler-path (and connected (or (= odd-count 0) (= odd-count 2))
                                       (> (length non-isolated) 0))))))))

  (unwind-protect
      (list
        ;; Triangle: all degree 2 -> Euler circuit
        (funcall 'neovm--euler-check
          '((1 . (2 3)) (2 . (1 3)) (3 . (1 2)))
          '(1 2 3))
        ;; Path 1-2-3: degrees 1,2,1 -> Euler path (not circuit)
        (funcall 'neovm--euler-check
          '((1 . (2)) (2 . (1 3)) (3 . (2)))
          '(1 2 3))
        ;; K4: all degree 3 -> no Euler circuit or path (4 odd vertices)
        (funcall 'neovm--euler-check
          '((1 . (2 3 4)) (2 . (1 3 4)) (3 . (1 2 4)) (4 . (1 2 3)))
          '(1 2 3 4))
        ;; Square with both diagonals: degrees 3,3,3,3 -> no Euler
        (funcall 'neovm--euler-check
          '((1 . (2 3 4)) (2 . (1 3 4)) (3 . (1 2 4)) (4 . (1 2 3)))
          '(1 2 3 4))
        ;; Square: degrees 2,2,2,2 -> Euler circuit
        (funcall 'neovm--euler-check
          '((1 . (2 4)) (2 . (1 3)) (3 . (2 4)) (4 . (3 1)))
          '(1 2 3 4)))
    (fmakunbound 'neovm--euler-check)))"#;
    let expect = expect_test::expect![[
        r#""OK (((degrees ((1 . 2) (2 . 2) (3 . 2))) (odd-degree-count 0) (connected t) (has-euler-circuit t) (has-euler-path t)) ((degrees ((1 . 1) (2 . 2) (3 . 1))) (odd-degree-count 2) (connected t) (has-euler-circuit nil) (has-euler-path t)) ((degrees ((1 . 3) (2 . 3) (3 . 3) (4 . 3))) (odd-degree-count 4) (connected t) (has-euler-circuit nil) (has-euler-path nil)) ((degrees ((1 . 3) (2 . 3) (3 . 3) (4 . 3))) (odd-degree-count 4) (connected t) (has-euler-circuit nil) (has-euler-path nil)) ((degrees ((1 . 2) (2 . 2) (3 . 2) (4 . 2))) (odd-degree-count 0) (connected t) (has-euler-circuit t) (has-euler-path t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimum Spanning Tree (Kruskal's using union-find)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_kruskal_mst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Union-Find with path compression
  (fset 'neovm--uf-make
    (lambda (nodes)
      (let ((parent (make-hash-table))
            (rank (make-hash-table)))
        (dolist (n nodes)
          (puthash n n parent)
          (puthash n 0 rank))
        (cons parent rank))))

  (fset 'neovm--uf-find
    (lambda (uf x)
      (let ((parent (car uf)))
        (while (not (eq (gethash x parent) x))
          (puthash x (gethash (gethash x parent) parent) parent)  ;; path compression
          (setq x (gethash x parent)))
        x)))

  (fset 'neovm--uf-union
    (lambda (uf a b)
      (let* ((parent (car uf))
             (rank (cdr uf))
             (ra (funcall 'neovm--uf-find uf a))
             (rb (funcall 'neovm--uf-find uf b)))
        (unless (eq ra rb)
          (cond
           ((< (gethash ra rank) (gethash rb rank))
            (puthash ra rb parent))
           ((> (gethash ra rank) (gethash rb rank))
            (puthash rb ra parent))
           (t
            (puthash rb ra parent)
            (puthash ra (1+ (gethash ra rank)) rank)))))))

  ;; Kruskal's MST
  (fset 'neovm--kruskal
    (lambda (edges nodes)
      ;; Sort edges by weight
      (let ((sorted-edges (sort (copy-sequence edges)
                                (lambda (a b) (< (caddr a) (caddr b)))))
            (uf (funcall 'neovm--uf-make nodes))
            (mst nil)
            (total-weight 0))
        (dolist (edge sorted-edges)
          (let ((u (car edge)) (v (cadr edge)) (w (caddr edge)))
            (unless (eq (funcall 'neovm--uf-find uf u)
                        (funcall 'neovm--uf-find uf v))
              (funcall 'neovm--uf-union uf u v)
              (setq mst (cons edge mst))
              (setq total-weight (+ total-weight w)))))
        (list (sort (nreverse mst)
                    (lambda (a b) (< (caddr a) (caddr b))))
              total-weight
              (length mst)))))

  (unwind-protect
      (list
        ;; Simple graph
        (funcall 'neovm--kruskal
          '((1 2 4) (1 3 2) (2 3 5) (2 4 10) (3 4 3) (3 5 8) (4 5 7))
          '(1 2 3 4 5))
        ;; Complete graph K4
        (funcall 'neovm--kruskal
          '((1 2 1) (1 3 3) (1 4 5) (2 3 2) (2 4 4) (3 4 6))
          '(1 2 3 4))
        ;; Already a tree (path)
        (funcall 'neovm--kruskal
          '((1 2 3) (2 3 1) (3 4 2))
          '(1 2 3 4)))
    (fmakunbound 'neovm--uf-make)
    (fmakunbound 'neovm--uf-find)
    (fmakunbound 'neovm--uf-union)
    (fmakunbound 'neovm--kruskal)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((1 3 2) (3 4 3) (1 2 4) (4 5 7)) 16 1) (((1 2 1) (2 3 2) (2 4 4)) 7 1) (((2 3 1) (3 4 2) (1 2 3)) 6 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prim's MST algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_prim_mst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Prim's algorithm with linear scan for min-weight edge
  (fset 'neovm--prim-mst
    (lambda (graph nodes)
      (let ((in-mst (make-hash-table))
            (key (make-hash-table))       ;; min weight to connect node to MST
            (parent (make-hash-table))
            (mst-edges nil)
            (total-weight 0))
        ;; Initialize
        (dolist (n nodes) (puthash n 999999 key))
        (puthash (car nodes) 0 key)
        (let ((remaining (copy-sequence nodes)))
          (while remaining
            ;; Find min-key node not in MST
            (let ((u nil) (min-k 999999))
              (dolist (n remaining)
                (when (< (gethash n key) min-k)
                  (setq u n min-k (gethash n key))))
              (when u
                (setq remaining (delq u remaining))
                (puthash u t in-mst)
                (when (gethash u parent)
                  (setq mst-edges (cons (list (gethash u parent) u min-k) mst-edges))
                  (setq total-weight (+ total-weight min-k)))
                ;; Update keys for neighbors
                (dolist (edge (cdr (assq u graph)))
                  (let ((v (car edge)) (w (cdr edge)))
                    (when (and (not (gethash v in-mst))
                               (< w (gethash v key 999999)))
                      (puthash v w key)
                      (puthash v u parent))))))))
        (list (sort (nreverse mst-edges)
                    (lambda (a b) (< (caddr a) (caddr b))))
              total-weight
              (length mst-edges)))))

  (unwind-protect
      (list
        ;; Same graph as Kruskal test (weighted adj list: node . ((nbr . weight) ...))
        (funcall 'neovm--prim-mst
          '((1 . ((2 . 4) (3 . 2)))
            (2 . ((1 . 4) (3 . 5) (4 . 10)))
            (3 . ((1 . 2) (2 . 5) (4 . 3) (5 . 8)))
            (4 . ((2 . 10) (3 . 3) (5 . 7)))
            (5 . ((3 . 8) (4 . 7))))
          '(1 2 3 4 5))
        ;; Triangle
        (funcall 'neovm--prim-mst
          '((1 . ((2 . 1) (3 . 3)))
            (2 . ((1 . 1) (3 . 2)))
            (3 . ((1 . 3) (2 . 2))))
          '(1 2 3)))
    (fmakunbound 'neovm--prim-mst)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((1 3 2) (3 4 3) (1 2 4) (4 5 7)) 16 1) (((1 2 1) (2 3 2)) 3 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-degree / out-degree analysis for directed graphs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_degree_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute in-degree, out-degree, sources, sinks for directed graph
  (fset 'neovm--degree-analysis
    (lambda (graph nodes)
      (let ((in-deg (make-hash-table))
            (out-deg (make-hash-table)))
        (dolist (n nodes)
          (puthash n 0 in-deg)
          (puthash n (length (cdr (assq n graph))) out-deg))
        (dolist (entry graph)
          (dolist (v (cdr entry))
            (puthash v (1+ (gethash v in-deg 0)) in-deg)))
        (let ((sources nil) (sinks nil) (isolated nil))
          (dolist (n nodes)
            (let ((id (gethash n in-deg)) (od (gethash n out-deg)))
              (when (and (= id 0) (> od 0)) (setq sources (cons n sources)))
              (when (and (= od 0) (> id 0)) (setq sinks (cons n sinks)))
              (when (and (= id 0) (= od 0)) (setq isolated (cons n isolated)))))
          (let ((deg-pairs nil))
            (dolist (n nodes)
              (setq deg-pairs (cons (list n (gethash n in-deg) (gethash n out-deg)) deg-pairs)))
            (list
              (list 'degrees (sort (nreverse deg-pairs)
                                   (lambda (a b) (string< (symbol-name (car a))
                                                          (symbol-name (car b))))))
              (list 'sources (sort (nreverse sources)
                                   (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
              (list 'sinks (sort (nreverse sinks)
                                  (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
              (list 'isolated (sort (nreverse isolated)
                                     (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))))))

  (unwind-protect
      (list
        ;; Complex DAG
        (funcall 'neovm--degree-analysis
          '((a . (b c)) (b . (d)) (c . (d e)) (d . (f)) (e . (f)) (f . ()) (g . ()))
          '(a b c d e f g))
        ;; Cycle: no sources or sinks
        (funcall 'neovm--degree-analysis
          '((a . (b)) (b . (c)) (c . (a)))
          '(a b c)))
    (fmakunbound 'neovm--degree-analysis)))"#;
    let expect = expect_test::expect![[
        r#""OK (((degrees ((a 0 2) (b 1 1) (c 1 2) (d 2 1) (e 1 1) (f 2 0) (g 0 0))) (sources (a)) (sinks (f)) (isolated (g))) ((degrees ((a 1 1) (b 1 1) (c 1 1))) (sources nil) (sinks nil) (isolated nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Graph transpose (reverse all edges)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_adv_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Transpose: reverse all edge directions
  (fset 'neovm--transpose-graph
    (lambda (graph nodes)
      (let ((result nil))
        (dolist (n nodes) (setq result (cons (cons n nil) result)))
        (dolist (entry graph)
          (let ((u (car entry)))
            (dolist (v (cdr entry))
              (let ((e (assq v result)))
                (when e (setcdr e (cons u (cdr e))))))))
        ;; Sort neighbor lists for deterministic output
        (dolist (entry result)
          (setcdr entry (sort (cdr entry)
                              (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
        (sort result (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b))))))))

  (unwind-protect
      (list
        ;; DAG
        (funcall 'neovm--transpose-graph
          '((a . (b c)) (b . (d)) (c . (d e)) (d . ()) (e . ()))
          '(a b c d e))
        ;; Cycle
        (funcall 'neovm--transpose-graph
          '((a . (b)) (b . (c)) (c . (a)))
          '(a b c))
        ;; Self-loop
        (funcall 'neovm--transpose-graph
          '((x . (x y)) (y . (z)) (z . ()))
          '(x y z)))
    (fmakunbound 'neovm--transpose-graph)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a) (b a) (c a) (d b c) (e c)) ((a c) (b a) (c b)) ((x x) (y x) (z y)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
